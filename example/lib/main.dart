import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:stow/stow.dart' as stow;

void main() {
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  String _status = 'Initializing Stow...';
  Uint8List? _syncBytes;
  Uint8List? _asyncBytes;
  bool _removed = false;
  int _boxCount = 0;

  @override
  void initState() {
    super.initState();
    _runStowDemo();
  }

  Future<void> _runStowDemo() async {
    try {
      // 1. Initialize box
      final box = await stow.initialize('demo_box');

      // 2. Write raw byte data (e.g. 0x01 through 0x08)
      final sampleData = Uint8List.fromList([1, 2, 4, 8, 16, 32, 64, 128]);
      await box.write('sample_bytes', sampleData);

      // 3. Synchronous read
      final syncRead = box.readSync('sample_bytes');

      // 4. Asynchronous read
      final asyncRead = await box.readAsync('sample_bytes');

      // 5. Box length
      final count = box.length;

      // 6. Asynchronous remove
      final removed = await box.remove('sample_bytes');

      setState(() {
        _status = 'Stow Demo Completed Successfully!';
        _syncBytes = syncRead;
        _asyncBytes = asyncRead;
        _removed = removed;
        _boxCount = count;
      });
    } catch (e) {
      setState(() {
        _status = 'Error: $e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    const titleStyle = TextStyle(fontSize: 22, fontWeight: FontWeight.bold);
    const bodyStyle = TextStyle(fontSize: 16);
    const codeStyle = TextStyle(
      fontFamily: 'monospace',
      fontSize: 15,
      fontWeight: FontWeight.w600,
    );
    const spacer = SizedBox(height: 12);

    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(
          title: const Text('Stow Local Storage (Rust + FFI)'),
          backgroundColor: Colors.indigo,
        ),
        body: SingleChildScrollView(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(_status, style: titleStyle),
              spacer,
              const Divider(),
              spacer,
              const Text('1. Box initialized: "demo_box"', style: bodyStyle),
              spacer,
              Text('2. Entries count before removal: $_boxCount', style: bodyStyle),
              spacer,
              Text(
                '3. readSync("sample_bytes"): ${_syncBytes != null ? _syncBytes.toString() : "null"}',
                style: codeStyle,
              ),
              spacer,
              Text(
                '4. readAsync("sample_bytes"): ${_asyncBytes != null ? _asyncBytes.toString() : "null"}',
                style: codeStyle,
              ),
              spacer,
              Text(
                '5. remove("sample_bytes"): ${_removed ? "Success (true)" : "Failed"}',
                style: bodyStyle,
              ),
              spacer,
              ElevatedButton.icon(
                onPressed: _runStowDemo,
                icon: const Icon(Icons.refresh),
                label: const Text('Re-run Demo'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
