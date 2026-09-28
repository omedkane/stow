import 'dart:async';
import 'dart:convert';
import 'dart:ffi' as ffi;
import 'dart:isolate';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'stow_bindings_generated.dart';

/// A blazing fast native key-value local storage engine for Flutter and Dart.
///
/// Powered by Rust via FFI, storing and reading raw bytes directly with
/// zero JSON or UTF-8 overhead.
class Stow {
  /// The name of this storage box.
  final String boxName;

  /// The custom directory path for this box, if provided.
  final String? path;

  int _handle;
  bool _isClosed = false;

  Stow._(this.boxName, this.path, this._handle);

  /// Creates a Stow box handle. Call [initialize] or [open] to initialize it.
  Stow([String? name]) : boxName = name ?? 'default', path = null, _handle = 0;

  static final Map<String, Stow> _openBoxes = <String, Stow>{};
  static Stow? _defaultBox;

  /// Returns the default or most recently initialized [Stow] box.
  static Stow get instance {
    final box = _defaultBox;
    if (box == null) {
      throw StowException(
        'Stow has not been initialized. Call Stow.initialize("boxName") first.',
      );
    }
    return box;
  }

  /// Returns an already opened [Stow] box by [boxName], or `null` if not opened.
  static Stow? box(String boxName) => _openBoxes[boxName];

  /// Initializes or opens a box named [boxName].
  ///
  /// Optionally accepts [path] to specify the directory where the database file
  /// will be stored. If omitted, Stow defaults to `./.stow`.
  static Future<Stow> initialize(String boxName, {String? path}) async {
    if (_openBoxes.containsKey(boxName)) {
      final existing = _openBoxes[boxName]!;
      _defaultBox = existing;
      return existing;
    }

    final handle = await Isolate.run(() {
      return using((arena) {
        final namePtr = boxName.toNativeUtf8(allocator: arena);
        final pathPtr = path != null
            ? path.toNativeUtf8(allocator: arena)
            : ffi.nullptr.cast<ffi.Char>();
        final outHandle = arena<ffi.Uint64>();

        final res = nativeStowInitBox(
          namePtr.cast(),
          pathPtr.cast(),
          outHandle,
        );
        if (res < 0) {
          throw StowException(
            'Failed to initialize box "$boxName" (code: $res, error: ${_getErrorMessage(res)})',
          );
        }
        return outHandle.value;
      });
    });

    final newBox = Stow._(boxName, path, handle);
    _openBoxes[boxName] = newBox;
    _defaultBox = newBox;
    return newBox;
  }

  /// Instance method to initialize this box.
  Future<Stow> open({String? customPath}) async {
    final opened = await Stow.initialize(boxName, path: customPath ?? path);
    _handle = opened._handle;
    _isClosed = false;
    return opened;
  }

  /// Synchronously reads the raw bytes for [key].
  ///
  /// Returns `null` if the key does not exist.
  Uint8List? readSync(String key) {
    _ensureOpen();
    final keyBytes = utf8.encode(key);
    return using((arena) {
      final keyPtr = arena<ffi.Uint8>(keyBytes.length);
      keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);
      final outValPtr = arena<ffi.Pointer<ffi.Uint8>>();
      final outValLen = arena<ffi.Size>();

      final res = nativeStowRead(
        _handle,
        keyPtr,
        keyBytes.length,
        outValPtr,
        outValLen,
      );

      if (res < 0) {
        throw StowException('stow_read failed: ${_getErrorMessage(res)}');
      }
      if (res == 0 || outValPtr.value == ffi.nullptr) {
        return null;
      }

      final ptr = outValPtr.value;
      final len = outValLen.value;
      final bytes = Uint8List.fromList(ptr.asTypedList(len));
      nativeStowFreeBytes(ptr, len);
      return bytes;
    });
  }

  /// Asynchronously reads the raw bytes for [key] in a background isolate,
  /// keeping the main UI thread completely unblocked.
  ///
  /// Returns `null` if the key does not exist.
  Future<Uint8List?> readAsync(String key) async {
    _ensureOpen();
    final handle = _handle;
    return Isolate.run(() {
      final keyBytes = utf8.encode(key);
      return using((arena) {
        final keyPtr = arena<ffi.Uint8>(keyBytes.length);
        keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);
        final outValPtr = arena<ffi.Pointer<ffi.Uint8>>();
        final outValLen = arena<ffi.Size>();

        final res = nativeStowRead(
          handle,
          keyPtr,
          keyBytes.length,
          outValPtr,
          outValLen,
        );

        if (res < 0) {
          throw StowException('stow_read failed with code: $res');
        }
        if (res == 0 || outValPtr.value == ffi.nullptr) {
          return null;
        }

        final ptr = outValPtr.value;
        final len = outValLen.value;
        final bytes = Uint8List.fromList(ptr.asTypedList(len));
        nativeStowFreeBytes(ptr, len);
        return bytes;
      });
    });
  }

  /// Asynchronously writes raw [value] bytes for [key].
  Future<void> write(String key, Uint8List value) async {
    _ensureOpen();
    final handle = _handle;
    await Isolate.run(() {
      final keyBytes = utf8.encode(key);
      using((arena) {
        final keyPtr = arena<ffi.Uint8>(keyBytes.length);
        keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);
        final valPtr = arena<ffi.Uint8>(value.length);
        valPtr.asTypedList(value.length).setAll(0, value);

        final res = nativeStowWrite(
          handle,
          keyPtr,
          keyBytes.length,
          valPtr,
          value.length,
        );

        if (res < 0) {
          throw StowException('stow_write failed with code: $res');
        }
      });
    });
  }

  /// Synchronously writes raw [value] bytes for [key].
  void writeSync(String key, Uint8List value) {
    _ensureOpen();
    final keyBytes = utf8.encode(key);
    using((arena) {
      final keyPtr = arena<ffi.Uint8>(keyBytes.length);
      keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);
      final valPtr = arena<ffi.Uint8>(value.length);
      valPtr.asTypedList(value.length).setAll(0, value);

      final res = nativeStowWrite(
        _handle,
        keyPtr,
        keyBytes.length,
        valPtr,
        value.length,
      );

      if (res < 0) {
        throw StowException('stow_write failed: ${_getErrorMessage(res)}');
      }
    });
  }

  /// Asynchronously removes [key] from the box.
  ///
  /// Returns `true` if the key was found and removed, `false` otherwise.
  Future<bool> remove(String key) async {
    _ensureOpen();
    final handle = _handle;
    return Isolate.run(() {
      final keyBytes = utf8.encode(key);
      return using((arena) {
        final keyPtr = arena<ffi.Uint8>(keyBytes.length);
        keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);

        final res = nativeStowRemove(handle, keyPtr, keyBytes.length);

        if (res < 0) {
          throw StowException('stow_remove failed with code: $res');
        }
        return res == 1;
      });
    });
  }

  /// Synchronously removes [key] from the box.
  ///
  /// Returns `true` if the key was found and removed, `false` otherwise.
  bool removeSync(String key) {
    _ensureOpen();
    final keyBytes = utf8.encode(key);
    return using((arena) {
      final keyPtr = arena<ffi.Uint8>(keyBytes.length);
      keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);

      final res = nativeStowRemove(_handle, keyPtr, keyBytes.length);

      if (res < 0) {
        throw StowException('stow_remove failed: ${_getErrorMessage(res)}');
      }
      return res == 1;
    });
  }

  /// Checks if [key] exists in the box.
  bool contains(String key) {
    _ensureOpen();
    final keyBytes = utf8.encode(key);
    return using((arena) {
      final keyPtr = arena<ffi.Uint8>(keyBytes.length);
      keyPtr.asTypedList(keyBytes.length).setAll(0, keyBytes);
      final res = nativeStowContains(_handle, keyPtr, keyBytes.length);
      return res == 1;
    });
  }

  /// Returns the number of entries stored in the box.
  int get length {
    _ensureOpen();
    final count = nativeStowKeysCount(_handle);
    if (count < 0) {
      throw StowException('stow_keys_count failed with code: $count');
    }
    return count;
  }

  /// Returns all stored keys in the box.
  List<String> get keys {
    _ensureOpen();
    return using((arena) {
      final outKeysPtr = arena<ffi.Pointer<ffi.Uint8>>();
      final outKeysLen = arena<ffi.Size>();

      final res = nativeStowGetAllKeys(_handle, outKeysPtr, outKeysLen);
      if (res < 0 || outKeysPtr.value == ffi.nullptr) {
        return <String>[];
      }

      final ptr = outKeysPtr.value;
      final totalLen = outKeysLen.value;
      final byteData = ptr.asTypedList(totalLen).buffer.asByteData();

      var offset = 0;
      final count = byteData.getUint32(offset, Endian.little);
      offset += 4;

      final result = <String>[];
      for (var i = 0; i < count; i++) {
        final kLen = byteData.getUint32(offset, Endian.little);
        offset += 4;
        final kBytes = ptr.asTypedList(totalLen).sublist(offset, offset + kLen);
        offset += kLen;
        result.add(utf8.decode(kBytes));
      }

      nativeStowFreeBytes(ptr, totalLen);
      return result;
    });
  }

  /// Clears all entries from this box.
  Future<void> clear() async {
    _ensureOpen();
    final handle = _handle;
    await Isolate.run(() {
      final res = nativeStowClear(handle);
      if (res < 0) {
        throw StowException('stow_clear failed with code: $res');
      }
    });
  }

  /// Compacts the box database file to reclaim disk space from deleted/overwritten entries.
  Future<void> compact() async {
    _ensureOpen();
    final handle = _handle;
    await Isolate.run(() {
      final res = nativeStowCompact(handle);
      if (res < 0) {
        throw StowException('stow_compact failed with code: $res');
      }
    });
  }

  /// Closes the box.
  Future<void> close() async {
    if (_isClosed) return;
    _isClosed = true;
    _openBoxes.remove(boxName);
    if (_defaultBox == this) {
      _defaultBox = _openBoxes.values.firstOrNull;
    }
    final handle = _handle;
    await Isolate.run(() {
      nativeStowClose(handle);
    });
  }

  void _ensureOpen() {
    if (_isClosed || _handle == 0) {
      throw StowException('Box "$boxName" is not open or has been closed.');
    }
  }

  static String _getErrorMessage(int code) {
    return using((arena) {
      final outPtr = arena<ffi.Pointer<ffi.Char>>();
      final res = nativeStowLastError(outPtr);
      if (res == 0 && outPtr.value != ffi.nullptr) {
        final str = outPtr.value.cast<Utf8>().toDartString();
        nativeStowFreeString(outPtr.value);
        return str;
      }
      return 'Native error code: $code';
    });
  }
}

// ---------------------------------------------------------------------------
// Top-Level Convenience API
// ---------------------------------------------------------------------------

/// Initializes or opens a [Stow] box named [boxName].
Future<Stow> initialize(String boxName, {String? path}) =>
    Stow.initialize(boxName, path: path);

/// Synchronously reads raw bytes for [key] from the default/active box.
Uint8List? readSync(String key) => Stow.instance.readSync(key);

/// Asynchronously reads raw bytes for [key] from the default/active box.
Future<Uint8List?> readAsync(String key) => Stow.instance.readAsync(key);

/// Asynchronously writes raw [value] bytes for [key] to the default/active box.
Future<void> write(String key, Uint8List value) =>
    Stow.instance.write(key, value);

/// Asynchronously removes [key] from the default/active box.
Future<bool> remove(String key) => Stow.instance.remove(key);

/// Exception thrown on Stow storage errors.
class StowException implements Exception {
  /// Description of the error.
  final String message;

  StowException(this.message);

  @override
  String toString() => 'StowException: $message';
}
