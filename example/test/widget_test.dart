import 'package:flutter_test/flutter_test.dart';
import 'package:stow_example/main.dart';

void main() {
  testWidgets('Stow example app smoke test', (WidgetTester tester) async {
    await tester.runAsync(() async {
      await tester.pumpWidget(const MyApp());
      await Future<void>.delayed(const Duration(milliseconds: 1000));
    });
    await tester.pump();

    expect(find.text('Stow Local Storage (Rust + FFI)'), findsOneWidget);
    expect(find.text('Stow Demo Completed Successfully!'), findsOneWidget);
    expect(find.text('5. remove("sample_bytes"): Success (true)'), findsOneWidget);
  });
}
