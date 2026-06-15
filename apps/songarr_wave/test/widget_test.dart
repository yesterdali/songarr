import 'package:flutter_test/flutter_test.dart';
import 'package:songarr_wave/main.dart';

void main() {
  testWidgets('shows the login screen', (tester) async {
    await tester.pumpWidget(const SongarrWaveApp());
    await tester.pump();

    expect(find.text('Твоя волна'), findsOneWidget);
    expect(find.text('Songarr URL'), findsOneWidget);
    expect(find.text('Log in'), findsOneWidget);
  });
}
