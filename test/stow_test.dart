import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:stow/stow.dart' as stow;

void main() {
  late Directory tempDir;

  setUp(() async {
    tempDir = await Directory.systemTemp.createTemp('stow_dart_test_');
  });

  tearDown(() async {
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });

  test('initialize, write async, readSync, readAsync, remove async with raw bytes', () async {
    final box = await stow.initialize('test_box_1', path: tempDir.path);

    expect(box.boxName, equals('test_box_1'));
    expect(box.length, equals(0));

    // Arbitrary binary bytes (including zeroes, 255, non-utf8 sequence)
    final rawBytes = Uint8List.fromList([0x00, 0xFF, 0xFE, 0x80, 0x7F, 0x42, 0x13, 0x37]);
    const key = 'binary_data_key';

    // 1. Write asynchronous
    await box.write(key, rawBytes);
    expect(box.length, equals(1));
    expect(box.contains(key), isTrue);

    // 2. readSync
    final syncResult = box.readSync(key);
    expect(syncResult, isNotNull);
    expect(syncResult, equals(rawBytes));

    // 3. readAsync
    final asyncResult = await box.readAsync(key);
    expect(asyncResult, isNotNull);
    expect(asyncResult, equals(rawBytes));

    // 4. Non-existent key returns null
    expect(box.readSync('non_existent_key'), isNull);
    expect(await box.readAsync('non_existent_key'), isNull);

    // 5. Remove asynchronous
    final removed = await box.remove(key);
    expect(removed, isTrue);
    expect(box.length, equals(0));
    expect(box.contains(key), isFalse);
    expect(box.readSync(key), isNull);
    expect(await box.readAsync(key), isNull);

    // 6. Removing non-existent key returns false
    final removedAgain = await box.remove(key);
    expect(removedAgain, isFalse);
  });

  test('top-level API works as expected', () async {
    await stow.initialize('top_level_box', path: tempDir.path);

    final payload = Uint8List.fromList([10, 20, 30, 40, 50]);
    await stow.write('cfg_payload', payload);

    final syncRead = stow.readSync('cfg_payload');
    expect(syncRead, equals(payload));

    final asyncRead = await stow.readAsync('cfg_payload');
    expect(asyncRead, equals(payload));

    final didRemove = await stow.remove('cfg_payload');
    expect(didRemove, isTrue);
    expect(stow.readSync('cfg_payload'), isNull);
  });

  test('multi-box isolation and keys iteration', () async {
    final boxA = await stow.initialize('box_a', path: tempDir.path);
    final boxB = await stow.initialize('box_b', path: tempDir.path);

    final bytesA = Uint8List.fromList([1, 1, 1]);
    final bytesB = Uint8List.fromList([2, 2, 2]);

    await boxA.write('shared_key', bytesA);
    await boxB.write('shared_key', bytesB);

    expect(boxA.readSync('shared_key'), equals(bytesA));
    expect(boxB.readSync('shared_key'), equals(bytesB));

    expect(boxA.keys, contains('shared_key'));
    expect(boxB.keys, contains('shared_key'));
  });

  test('data persistence across re-opening box', () async {
    final box1 = await stow.initialize('persisted_box', path: tempDir.path);
    final data = Uint8List.fromList([99, 88, 77, 66]);
    await box1.write('state', data);
    await box1.close();

    // Re-initialize box with same path
    final box2 = await stow.initialize('persisted_box', path: tempDir.path);
    expect(box2.readSync('state'), equals(data));
  });

  test('clear and compact work correctly', () async {
    final box = await stow.initialize('maintenance_box', path: tempDir.path);

    await box.write('k1', Uint8List.fromList([1]));
    await box.write('k2', Uint8List.fromList([2]));
    expect(box.length, equals(2));

    await box.compact();
    expect(box.length, equals(2));
    expect(box.readSync('k1'), equals(Uint8List.fromList([1])));

    await box.clear();
    expect(box.length, equals(0));
    expect(box.readSync('k1'), isNull);
  });
}
