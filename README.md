# Stow 📦⚡

A blazing fast native local key-value storage engine for Flutter and Dart, powered by **Rust** and **FFI**.

- **Raw bytes in, raw bytes out**: No JSON, UTF-8, or serialization overhead.
- **Zero manual build scripts**: Built with Dart Native Assets and build hooks (`hook/build.dart`). When you run `flutter run`, `flutter test`, `dart test`, or `flutter build`, Rust is compiled automatically.
- **Pure native performance**: Sub-microsecond synchronous reads, non-blocking asynchronous writes and reads, crash-resilient append-only WAL with CRC-32 integrity verification and compaction.
- **Native-first**: Supports macOS, iOS, Android, Linux, and Windows.

---

## Installation

Add `stow` to your `pubspec.yaml`:

```yaml
dependencies:
  stow:
    path: /path/to/stow # or from git / pub
```

---

## Quick Start

```dart
import 'dart:typed_data';
import 'package:stow/stow.dart' as stow;

void main() async {
  // 1. Initialize a box
  final box = await stow.initialize('my_box');

  // 2. Write raw bytes (asynchronous)
  final data = Uint8List.fromList([1, 2, 4, 8, 16, 32]);
  await box.write('auth_token', data);

  // 3. Synchronous read (O(1) memory lookup)
  final Uint8List? cachedToken = box.readSync('auth_token');

  // 4. Asynchronous read (offloaded to background isolate)
  final Uint8List? asyncToken = await box.readAsync('auth_token');

  // 5. Remove (asynchronous)
  await box.remove('auth_token');
}
```

---

## API Reference

### Top-Level & Box Methods

| Method | Type | Description |
|---|---|---|
| `initialize(String boxName, {String? path})` | `Future<Stow>` | Initializes or opens a box. |
| `readSync(String key)` | `Uint8List?` | Direct synchronous read of raw bytes. |
| `readAsync(String key)` | `Future<Uint8List?>` | Asynchronous read offloaded from UI thread. |
| `write(String key, Uint8List value)` | `Future<void>` | Asynchronous persistent write to WAL and index. |
| `remove(String key)` | `Future<bool>` | Asynchronous removal of key from box. |
| `writeSync(String key, Uint8List value)` | `void` | Synchronous write to WAL and index. |
| `removeSync(String key)` | `bool` | Synchronous removal of key from box. |
| `contains(String key)` | `bool` | Checks if a key exists in the box. |
| `length` | `int` | Number of entries stored in the box. |
| `keys` | `List<String>` | List of all stored keys. |
| `clear()` | `Future<void>` | Clears all data in the box. |
| `compact()` | `Future<void>` | Reclaims disk space from deleted/updated records. |
| `close()` | `Future<void>` | Closes the box. |

---

## How It Works

1. **Storage Architecture**:
   - Uses an append-only write-ahead log (WAL) on disk for durability.
   - Maintains an in-memory index protected by concurrent read-write locks (`RwLock`) for instant $O(1)$ reads.
   - CRC-32 checksums on every record ensure data integrity with automatic recovery on unclean exits.
2. **Build Automation**:
   - Uses Dart's native build hook (`hook/build.dart`) with `native_toolchain_rust`.
   - Compiles Rust code automatically on demand when building or testing the app.
   - Zero manual shell scripts or multi-step cargo copy commands required.


* For Android: Gradle, which invokes the Android NDK for native builds.
  * See the documentation in android/build.gradle.
* For iOS and MacOS: Xcode, via CocoaPods.
  * See the documentation in ios/stow.podspec.
  * See the documentation in macos/stow.podspec.
* For Linux and Windows: CMake.
  * See the documentation in linux/CMakeLists.txt.
  * See the documentation in windows/CMakeLists.txt.

## Binding to native code

To use the native code, bindings in Dart are needed.
To avoid writing these by hand, they are generated from the header file
(`src/stow.h`) by `package:ffigen`.
Regenerate the bindings by running `dart run ffigen --config ffigen.yaml`.

## Invoking native code

Very short-running native functions can be directly invoked from any isolate.
For example, see `sum` in `lib/stow.dart`.

Longer-running functions should be invoked on a helper isolate to avoid
dropping frames in Flutter applications.
For example, see `sumAsync` in `lib/stow.dart`.

## Flutter help

For help getting started with Flutter, view our
[online documentation](https://docs.flutter.dev), which offers tutorials,
samples, guidance on mobile development, and a full API reference.

The plugin project was generated without specifying the `--platforms` flag, so no platforms are currently supported.
To add platforms, run `flutter create -t plugin_ffi --platforms <platforms> .` in this directory.
You can also find a detailed instruction on how to add platforms in the `pubspec.yaml` at https://flutter.dev/to/pubspec-plugin-platforms.
