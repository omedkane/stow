#ifndef STOW_H
#define STOW_H

#include <stdint.h>
#include <stddef.h>

#if defined(_WIN32)
  #define STOW_EXPORT __declspec(dllexport)
#else
  #define STOW_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

/// Initializes or opens a box with [name] in [path] (optional).
/// Returns 0 on success, and writes the box handle to [*out_handle].
/// Returns negative value on error.
STOW_EXPORT int32_t stow_init_box(
    const char *name,
    const char *path,
    uint64_t *out_handle
);

/// Synchronously reads a value by [key] from the box specified by [handle].
/// If found, returns 1, writes pointer to [*out_val_ptr] and length to [*out_val_len].
/// The returned buffer must be freed using [stow_free_bytes].
/// If not found, returns 0.
/// Returns negative value on error.
STOW_EXPORT int32_t stow_read(
    uint64_t handle,
    const uint8_t *key_ptr,
    size_t key_len,
    uint8_t **out_val_ptr,
    size_t *out_val_len
);

/// Writes [val] for [key] to the box specified by [handle].
/// Returns 0 on success, negative value on error.
STOW_EXPORT int32_t stow_write(
    uint64_t handle,
    const uint8_t *key_ptr,
    size_t key_len,
    const uint8_t *val_ptr,
    size_t val_len
);

/// Removes [key] from the box specified by [handle].
/// Returns 1 if removed, 0 if not found, negative value on error.
STOW_EXPORT int32_t stow_remove(
    uint64_t handle,
    const uint8_t *key_ptr,
    size_t key_len
);

/// Checks if [key] exists in the box specified by [handle].
/// Returns 1 if key exists, 0 if not found, negative value on error.
STOW_EXPORT int32_t stow_contains(
    uint64_t handle,
    const uint8_t *key_ptr,
    size_t key_len
);

/// Returns the number of keys in the box specified by [handle].
/// Returns negative value on error.
STOW_EXPORT int64_t stow_keys_count(uint64_t handle);

/// Returns all keys in the box as packed bytes: count(u32), [len(u32), bytes]*
/// The returned buffer must be freed using [stow_free_bytes].
STOW_EXPORT int32_t stow_get_all_keys(
    uint64_t handle,
    uint8_t **out_keys_ptr,
    size_t *out_keys_len
);

/// Clears all keys from the box specified by [handle].
/// Returns 0 on success, negative value on error.
STOW_EXPORT int32_t stow_clear(uint64_t handle);

/// Compacts the database file to remove deleted/overwritten entries.
/// Returns 0 on success, negative value on error.
STOW_EXPORT int32_t stow_compact(uint64_t handle);

/// Closes the box specified by [handle].
/// Returns 0 on success, negative value on error.
STOW_EXPORT int32_t stow_close(uint64_t handle);

/// Frees a byte buffer allocated by stow.
STOW_EXPORT void stow_free_bytes(uint8_t *ptr, size_t len);

/// Frees a string allocated by stow.
STOW_EXPORT void stow_free_string(char *ptr);

/// Gets the last error message, if any.
/// The returned string must be freed using [stow_free_string].
STOW_EXPORT int32_t stow_last_error(char **out_ptr);

#ifdef __cplusplus
}
#endif

#endif // STOW_H
