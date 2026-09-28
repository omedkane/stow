use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

// Magic bytes for file header: "STOW_DB\0"
const FILE_MAGIC: &[u8; 8] = b"STOW_DB\0";
const FILE_VERSION: u32 = 1;

// Record magic: "ST"
const RECORD_MAGIC: u16 = 0x5354;
const OP_PUT: u8 = 1;
const OP_REMOVE: u8 = 2;
const OP_CLEAR: u8 = 3;

/// CRC32 IEEE 802.3 implementation
fn crc32(data: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for i in 0..256 {
            let mut crc = i as u32;
            for _ in 0..8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
            }
            table[i] = crc;
        }
        table
    });

    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let idx = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ table[idx];
    }
    !crc
}

struct BoxStorage {
    name: String,
    path: PathBuf,
    file: Mutex<File>,
    index: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    tombstones: Mutex<usize>,
}

impl BoxStorage {
    fn open(name: String, dir_path: Option<&str>) -> io::Result<Self> {
        let dir: PathBuf = match dir_path {
            Some(p) if !p.trim().is_empty() => PathBuf::from(p),
            _ => {
                if let Ok(env_dir) = std::env::var("STOW_DIR") {
                    PathBuf::from(env_dir)
                } else {
                    PathBuf::from(".stow")
                }
            }
        };

        fs::create_dir_all(&dir)?;
        let file_path = dir.join(format!("{}.stow", name));

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&file_path)?;

        let mut index = HashMap::new();
        let mut tombstones = 0usize;

        let file_len = file.metadata()?.len();
        if file_len == 0 {
            // Write fresh header
            file.write_all(FILE_MAGIC)?;
            file.write_all(&FILE_VERSION.to_le_bytes())?;
            file.write_all(&[0u8; 4])?; // Reserved
            file.flush()?;
        } else {
            // Validate header and replay records
            let mut header = [0u8; 16];
            if let Err(_) = file.read_exact(&mut header) {
                // Truncate and write fresh if header is broken
                file.set_len(0)?;
                file.seek(SeekFrom::Start(0))?;
                file.write_all(FILE_MAGIC)?;
                file.write_all(&FILE_VERSION.to_le_bytes())?;
                file.write_all(&[0u8; 4])?;
                file.flush()?;
                return Ok(Self {
                    name,
                    path: file_path,
                    file: Mutex::new(file),
                    index: RwLock::new(index),
                    tombstones: Mutex::new(0),
                });
            }

            if &header[0..8] != FILE_MAGIC {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid Stow database file format",
                ));
            }

            let mut last_valid_offset = 16u64;

            loop {
                let current_offset = match file.seek(SeekFrom::Start(last_valid_offset)) {
                    Ok(o) => o,
                    Err(_) => break,
                };

                let mut rec_header = [0u8; 12]; // magic(2), op(1), flags(1), key_len(4), val_len(4)
                if file.read_exact(&mut rec_header).is_err() {
                    // Reached EOF cleanly
                    break;
                }

                let magic = u16::from_le_bytes([rec_header[0], rec_header[1]]);
                if magic != RECORD_MAGIC {
                    break;
                }

                let op = rec_header[2];
                let key_len = u32::from_le_bytes([
                    rec_header[4],
                    rec_header[5],
                    rec_header[6],
                    rec_header[7],
                ]) as usize;
                let val_len = u32::from_le_bytes([
                    rec_header[8],
                    rec_header[9],
                    rec_header[10],
                    rec_header[11],
                ]) as usize;

                let payload_len = key_len + val_len;
                let mut payload = vec![0u8; payload_len];
                if file.read_exact(&mut payload).is_err() {
                    break;
                }

                let mut crc_buf = [0u8; 4];
                if file.read_exact(&mut crc_buf).is_err() {
                    break;
                }

                let stored_crc = u32::from_le_bytes(crc_buf);

                // Verify CRC: over rec_header[2..12] + payload
                let mut crc_data = Vec::with_capacity(10 + payload_len);
                crc_data.extend_from_slice(&rec_header[2..12]);
                crc_data.extend_from_slice(&payload);

                if crc32(&crc_data) != stored_crc {
                    break;
                }

                let key = payload[..key_len].to_vec();
                let val = payload[key_len..].to_vec();

                match op {
                    OP_PUT => {
                        if index.insert(key, val).is_some() {
                            tombstones += 1;
                        }
                    }
                    OP_REMOVE => {
                        if index.remove(&key).is_some() {
                            tombstones += 1;
                        }
                    }
                    OP_CLEAR => {
                        index.clear();
                        tombstones = 0;
                    }
                    _ => break,
                }

                last_valid_offset = current_offset + 12 + payload_len as u64 + 4;
            }

            // Truncate any incomplete trailing record
            if last_valid_offset < file_len {
                file.set_len(last_valid_offset)?;
            }
            file.seek(SeekFrom::Start(last_valid_offset))?;
        }

        Ok(Self {
            name,
            path: file_path,
            file: Mutex::new(file),
            index: RwLock::new(index),
            tombstones: Mutex::new(tombstones),
        })
    }

    fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
        let guard = self.index.read().ok()?;
        guard.get(key).cloned()
    }

    fn write(&self, key: &[u8], val: &[u8]) -> io::Result<()> {
        let key_len = key.len() as u32;
        let val_len = val.len() as u32;

        let mut rec_header = [0u8; 12];
        rec_header[0..2].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
        rec_header[2] = OP_PUT;
        rec_header[3] = 0;
        rec_header[4..8].copy_from_slice(&key_len.to_le_bytes());
        rec_header[8..12].copy_from_slice(&val_len.to_le_bytes());

        let mut crc_data = Vec::with_capacity(10 + key.len() + val.len());
        crc_data.extend_from_slice(&rec_header[2..12]);
        crc_data.extend_from_slice(key);
        crc_data.extend_from_slice(val);
        let crc = crc32(&crc_data);

        {
            let mut file_guard = self.file.lock().map_err(|_| {
                io::Error::new(io::ErrorKind::Other, "File mutex poisoned")
            })?;

            file_guard.seek(SeekFrom::End(0))?;
            file_guard.write_all(&rec_header)?;
            file_guard.write_all(key)?;
            file_guard.write_all(val)?;
            file_guard.write_all(&crc.to_le_bytes())?;
            file_guard.flush()?;
        }

        {
            let mut index_guard = self.index.write().map_err(|_| {
                io::Error::new(io::ErrorKind::Other, "Index rwlock poisoned")
            })?;

            if index_guard.insert(key.to_vec(), val.to_vec()).is_some() {
                if let Ok(mut t) = self.tombstones.lock() {
                    *t += 1;
                }
            }
        }

        Ok(())
    }

    fn remove(&self, key: &[u8]) -> io::Result<bool> {
        let mut index_guard = self.index.write().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "Index rwlock poisoned")
        })?;

        if !index_guard.contains_key(key) {
            return Ok(false);
        }

        let key_len = key.len() as u32;
        let val_len = 0u32;

        let mut rec_header = [0u8; 12];
        rec_header[0..2].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
        rec_header[2] = OP_REMOVE;
        rec_header[3] = 0;
        rec_header[4..8].copy_from_slice(&key_len.to_le_bytes());
        rec_header[8..12].copy_from_slice(&val_len.to_le_bytes());

        let mut crc_data = Vec::with_capacity(10 + key.len());
        crc_data.extend_from_slice(&rec_header[2..12]);
        crc_data.extend_from_slice(key);
        let crc = crc32(&crc_data);

        {
            let mut file_guard = self.file.lock().map_err(|_| {
                io::Error::new(io::ErrorKind::Other, "File mutex poisoned")
            })?;

            file_guard.seek(SeekFrom::End(0))?;
            file_guard.write_all(&rec_header)?;
            file_guard.write_all(key)?;
            file_guard.write_all(&crc.to_le_bytes())?;
            file_guard.flush()?;
        }

        index_guard.remove(key);

        if let Ok(mut t) = self.tombstones.lock() {
            *t += 1;
        }

        Ok(true)
    }

    fn contains(&self, key: &[u8]) -> bool {
        if let Ok(guard) = self.index.read() {
            guard.contains_key(key)
        } else {
            false
        }
    }

    fn keys_count(&self) -> usize {
        self.index.read().map(|g| g.len()).unwrap_or(0)
    }

    fn all_keys(&self) -> Vec<Vec<u8>> {
        if let Ok(guard) = self.index.read() {
            guard.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    fn clear(&self) -> io::Result<()> {
        let mut index_guard = self.index.write().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "Index rwlock poisoned")
        })?;

        let mut file_guard = self.file.lock().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "File mutex poisoned")
        })?;

        // Reset file to clean header
        file_guard.set_len(0)?;
        file_guard.seek(SeekFrom::Start(0))?;
        file_guard.write_all(FILE_MAGIC)?;
        file_guard.write_all(&FILE_VERSION.to_le_bytes())?;
        file_guard.write_all(&[0u8; 4])?;
        file_guard.flush()?;

        index_guard.clear();
        if let Ok(mut t) = self.tombstones.lock() {
            *t = 0;
        }

        Ok(())
    }

    fn compact(&self) -> io::Result<()> {
        let index_snapshot = {
            let guard = self.index.read().map_err(|_| {
                io::Error::new(io::ErrorKind::Other, "Index rwlock poisoned")
            })?;
            guard.clone()
        };

        let temp_path = self.path.with_extension("stow.tmp");
        {
            let mut temp_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)?;

            temp_file.write_all(FILE_MAGIC)?;
            temp_file.write_all(&FILE_VERSION.to_le_bytes())?;
            temp_file.write_all(&[0u8; 4])?;

            for (key, val) in &index_snapshot {
                let key_len = key.len() as u32;
                let val_len = val.len() as u32;

                let mut rec_header = [0u8; 12];
                rec_header[0..2].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
                rec_header[2] = OP_PUT;
                rec_header[3] = 0;
                rec_header[4..8].copy_from_slice(&key_len.to_le_bytes());
                rec_header[8..12].copy_from_slice(&val_len.to_le_bytes());

                let mut crc_data = Vec::with_capacity(10 + key.len() + val.len());
                crc_data.extend_from_slice(&rec_header[2..12]);
                crc_data.extend_from_slice(key);
                crc_data.extend_from_slice(val);
                let crc = crc32(&crc_data);

                temp_file.write_all(&rec_header)?;
                temp_file.write_all(key)?;
                temp_file.write_all(val)?;
                temp_file.write_all(&crc.to_le_bytes())?;
            }
            temp_file.flush()?;
        }

        // Atomically replace file
        let mut file_guard = self.file.lock().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "File mutex poisoned")
        })?;

        fs::rename(&temp_path, &self.path)?;

        let reopened = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)?;
        *file_guard = reopened;

        if let Ok(mut t) = self.tombstones.lock() {
            *t = 0;
        }

        Ok(())
    }
}

// Global registry of open boxes
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static BOXES: OnceLock<RwLock<HashMap<u64, Arc<BoxStorage>>>> = OnceLock::new();
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn boxes_registry() -> &'static RwLock<HashMap<u64, Arc<BoxStorage>>> {
    BOXES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn set_last_error(msg: String) {
    if let Ok(mut lock) = LAST_ERROR.lock() {
        *lock = Some(msg);
    }
}

fn get_box(handle: u64) -> Option<Arc<BoxStorage>> {
    let registry = boxes_registry().read().ok()?;
    registry.get(&handle).cloned()
}

// ---------------------------------------------------------------------------
// C ABI EXPORTS
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn stow_init_box(
    name: *const c_char,
    path: *const c_char,
    out_handle: *mut u64,
) -> i32 {
    if name.is_null() || out_handle.is_null() {
        set_last_error("Null pointer provided to stow_init_box".into());
        return -1;
    }

    let box_name = match CStr::from_ptr(name).to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_last_error(format!("Invalid UTF-8 in box name: {}", e));
            return -4;
        }
    };

    let dir_path = if !path.is_null() {
        match CStr::from_ptr(path).to_str() {
            Ok(s) if !s.is_empty() => Some(s),
            _ => None,
        }
    } else {
        None
    };

    // Check if box already open with same name and path
    {
        if let Ok(reg) = boxes_registry().read() {
            for (&h, b) in reg.iter() {
                if b.name == box_name {
                    *out_handle = h;
                    return 0;
                }
            }
        }
    }

    match BoxStorage::open(box_name, dir_path) {
        Ok(storage) => {
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut reg) = boxes_registry().write() {
                reg.insert(handle, Arc::new(storage));
                *out_handle = handle;
                0
            } else {
                set_last_error("Failed to acquire write lock on registry".into());
                -5
            }
        }
        Err(e) => {
            set_last_error(format!("Failed to open box: {}", e));
            -3
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_read(
    handle: u64,
    key_ptr: *const u8,
    key_len: usize,
    out_val_ptr: *mut *mut u8,
    out_val_len: *mut usize,
) -> i32 {
    if key_ptr.is_null() || out_val_ptr.is_null() || out_val_len.is_null() {
        set_last_error("Null pointer provided to stow_read".into());
        return -1;
    }

    let storage = match get_box(handle) {
        Some(s) => s,
        None => {
            set_last_error("Invalid box handle".into());
            return -2;
        }
    };

    let key = std::slice::from_raw_parts(key_ptr, key_len);
    match storage.read(key) {
        Some(val) => {
            let len = val.len();
            let mut boxed_slice = val.into_boxed_slice();
            let ptr = boxed_slice.as_mut_ptr();
            std::mem::forget(boxed_slice);

            *out_val_ptr = ptr;
            *out_val_len = len;
            1 // Found
        }
        None => {
            *out_val_ptr = std::ptr::null_mut();
            *out_val_len = 0;
            0 // Not found
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_write(
    handle: u64,
    key_ptr: *const u8,
    key_len: usize,
    val_ptr: *const u8,
    val_len: usize,
) -> i32 {
    if key_ptr.is_null() || (val_len > 0 && val_ptr.is_null()) {
        set_last_error("Null pointer provided to stow_write".into());
        return -1;
    }

    let storage = match get_box(handle) {
        Some(s) => s,
        None => {
            set_last_error("Invalid box handle".into());
            return -2;
        }
    };

    let key = std::slice::from_raw_parts(key_ptr, key_len);
    let val = if val_len > 0 {
        std::slice::from_raw_parts(val_ptr, val_len)
    } else {
        &[]
    };

    match storage.write(key, val) {
        Ok(_) => 0,
        Err(e) => {
            set_last_error(format!("Failed to write key: {}", e));
            -3
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_remove(
    handle: u64,
    key_ptr: *const u8,
    key_len: usize,
) -> i32 {
    if key_ptr.is_null() {
        set_last_error("Null pointer provided to stow_remove".into());
        return -1;
    }

    let storage = match get_box(handle) {
        Some(s) => s,
        None => {
            set_last_error("Invalid box handle".into());
            return -2;
        }
    };

    let key = std::slice::from_raw_parts(key_ptr, key_len);
    match storage.remove(key) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => {
            set_last_error(format!("Failed to remove key: {}", e));
            -3
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_contains(
    handle: u64,
    key_ptr: *const u8,
    key_len: usize,
) -> i32 {
    if key_ptr.is_null() {
        return -1;
    }

    let storage = match get_box(handle) {
        Some(s) => s,
        None => return -2,
    };

    let key = std::slice::from_raw_parts(key_ptr, key_len);
    if storage.contains(key) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_get_all_keys(
    handle: u64,
    out_keys_ptr: *mut *mut u8,
    out_keys_len: *mut usize,
) -> i32 {
    if out_keys_ptr.is_null() || out_keys_len.is_null() {
        return -1;
    }

    let storage = match get_box(handle) {
        Some(s) => s,
        None => return -2,
    };

    let keys = storage.all_keys();
    let count = keys.len() as u32;

    let mut buf = Vec::new();
    buf.extend_from_slice(&count.to_le_bytes());
    for k in keys {
        let k_len = k.len() as u32;
        buf.extend_from_slice(&k_len.to_le_bytes());
        buf.extend_from_slice(&k);
    }

    let len = buf.len();
    let mut boxed = buf.into_boxed_slice();
    *out_keys_ptr = boxed.as_mut_ptr();
    *out_keys_len = len;
    std::mem::forget(boxed);

    0
}

#[no_mangle]
pub unsafe extern "C" fn stow_keys_count(handle: u64) -> i64 {
    match get_box(handle) {
        Some(s) => s.keys_count() as i64,
        None => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_clear(handle: u64) -> i32 {
    let storage = match get_box(handle) {
        Some(s) => s,
        None => return -2,
    };

    match storage.clear() {
        Ok(_) => 0,
        Err(e) => {
            set_last_error(format!("Failed to clear box: {}", e));
            -3
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_compact(handle: u64) -> i32 {
    let storage = match get_box(handle) {
        Some(s) => s,
        None => return -2,
    };

    match storage.compact() {
        Ok(_) => 0,
        Err(e) => {
            set_last_error(format!("Failed to compact box: {}", e));
            -3
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_close(handle: u64) -> i32 {
    if let Ok(mut reg) = boxes_registry().write() {
        if reg.remove(&handle).is_some() {
            0
        } else {
            -2 // Not found
        }
    } else {
        -5
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_free_bytes(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, len, len));
    }
}

#[no_mangle]
pub unsafe extern "C" fn stow_last_error(out_ptr: *mut *mut c_char) -> i32 {
    if out_ptr.is_null() {
        return -1;
    }

    if let Ok(mut lock) = LAST_ERROR.lock() {
        if let Some(msg) = lock.take() {
            if let Ok(c_str) = CString::new(msg) {
                *out_ptr = c_str.into_raw();
                return 0;
            }
        }
    }

    *out_ptr = std::ptr::null_mut();
    -1
}

#[no_mangle]
pub unsafe extern "C" fn stow_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("stow_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let storage = BoxStorage::open("test_box".into(), Some(temp_dir.to_str().unwrap())).unwrap();

        assert_eq!(storage.keys_count(), 0);

        let k1 = b"key1";
        let v1 = b"value1_bytes";
        storage.write(k1, v1).unwrap();

        assert_eq!(storage.keys_count(), 1);
        assert!(storage.contains(k1));
        assert_eq!(storage.read(k1).unwrap(), v1);

        // Overwrite
        let v1_new = b"value1_updated";
        storage.write(k1, v1_new).unwrap();
        assert_eq!(storage.read(k1).unwrap(), v1_new);

        // Remove
        assert!(storage.remove(k1).unwrap());
        assert!(!storage.contains(k1));
        assert!(storage.read(k1).is_none());

        // Write multiple
        storage.write(b"k1", b"v1").unwrap();
        storage.write(b"k2", b"v2").unwrap();
        storage.compact().unwrap();

        assert_eq!(storage.read(b"k1").unwrap(), b"v1");
        assert_eq!(storage.read(b"k2").unwrap(), b"v2");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
