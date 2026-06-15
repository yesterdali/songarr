import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

// Gothic palette — mirrors the web theme's wave-* tokens.
const kCream = Color(0xfff3ecdd);
const kPink = Color(0xffc41e3a);
const kDeepPink = Color(0xff7a0c1f);
const kViolet = Color(0xff45122e);
const kBlack = Color(0xff070408);

/// Wide-window breakpoint: side nav rail above this, bottom dock below.
const double kDesktopBreakpoint = 760;

ThemeData buildTheme() {
  final base = ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    colorScheme: ColorScheme.fromSeed(
      seedColor: kPink,
      brightness: Brightness.dark,
      surface: kBlack,
    ),
    scaffoldBackgroundColor: Colors.transparent,
  );
  // Cormorant (incl. Cyrillic) for display/headings; system sans for body.
  final display = GoogleFonts.cormorantTextTheme(base.textTheme).copyWith(
    bodyLarge: base.textTheme.bodyLarge,
    bodyMedium: base.textTheme.bodyMedium,
    bodySmall: base.textTheme.bodySmall,
    labelLarge: base.textTheme.labelLarge,
    labelMedium: base.textTheme.labelMedium,
    labelSmall: base.textTheme.labelSmall,
  );
  return base.copyWith(textTheme: display);
}

/// Display serif for big headings (e.g. "Твоя волна", screen titles).
TextStyle serif({
  double fontSize = 28,
  FontWeight fontWeight = FontWeight.w700,
  Color color = kCream,
  double? letterSpacing,
}) {
  return GoogleFonts.cormorant(
    fontSize: fontSize,
    fontWeight: fontWeight,
    color: color,
    letterSpacing: letterSpacing,
  );
}

/// Candlelit gloom behind every screen: ember glow below, cold violet above,
/// a vignette to round it off. Mirrors the web `body::before`.
class AmbientBackground extends StatelessWidget {
  const AmbientBackground({required this.child, super.key});
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(color: kBlack),
      child: Stack(
        children: [
          const Positioned.fill(child: _AmbientGlow()),
          Positioned.fill(child: child),
        ],
      ),
    );
  }
}

class _AmbientGlow extends StatelessWidget {
  const _AmbientGlow();

  @override
  Widget build(BuildContext context) {
    return const IgnorePointer(
      child: DecoratedBox(
        decoration: BoxDecoration(
          gradient: RadialGradient(
            center: Alignment(0, -1.1),
            radius: 1.1,
            colors: [Color(0x5945122e), Colors.transparent],
            stops: [0, 0.65],
          ),
        ),
        child: DecoratedBox(
          decoration: BoxDecoration(
            gradient: RadialGradient(
              center: Alignment(0, 1.2),
              radius: 1.1,
              colors: [Color(0x387a0c1f), Colors.transparent],
              stops: [0, 0.6],
            ),
          ),
        ),
      ),
    );
  }
}

/// Small-caps section heading flanked by fading crimson rules: ── ТЕКСТ ──.
class SectionTitle extends StatelessWidget {
  const SectionTitle(this.text, {this.trailing, super.key});
  final String text;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Row(
        children: [
          const _Rule(flip: true),
          const SizedBox(width: 12),
          Text(
            text.toUpperCase(),
            style: const TextStyle(
              color: Color(0xaad8b7be),
              fontSize: 12,
              fontWeight: FontWeight.w800,
              letterSpacing: 3,
            ),
          ),
          const SizedBox(width: 12),
          const Expanded(child: _Rule()),
          if (trailing != null) ...[const SizedBox(width: 12), trailing!],
        ],
      ),
    );
  }
}

class _Rule extends StatelessWidget {
  const _Rule({this.flip = false});
  final bool flip;

  @override
  Widget build(BuildContext context) {
    final gradient = LinearGradient(
      begin: flip ? Alignment.centerRight : Alignment.centerLeft,
      end: flip ? Alignment.centerLeft : Alignment.centerRight,
      colors: const [Color(0x73c41e3a), Colors.transparent],
    );
    final line = Container(
      height: 1,
      decoration: BoxDecoration(gradient: gradient),
    );
    return flip ? SizedBox(width: 26, child: line) : line;
  }
}

/// Elongated barbed gothic cross (the hero watermark).
class GothicCrossPainter extends CustomPainter {
  const GothicCrossPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    final cx = size.width / 2;
    final cy = size.height / 2;
    final unit = size.shortestSide / 9;

    // One barbed arm pointing up, reused via rotation for all four directions.
    Path arm() {
      final p = Path();
      p.moveTo(0, 0);
      p.lineTo(-0.7 * unit, -3.5 * unit);
      p.quadraticBezierTo(0, -2.1 * unit, 0.7 * unit, -3.5 * unit);
      p.close();
      return p;
    }

    canvas.save();
    canvas.translate(cx, cy);
    // Long descending arm: stretch the bottom one.
    for (final angle in [0.0, 90.0, 180.0, 270.0]) {
      canvas.save();
      canvas.rotate(angle * 3.1415926 / 180);
      final scaleY = angle == 180 ? 1.7 : 1.0;
      canvas.scale(1, scaleY);
      canvas.drawPath(arm(), paint);
      canvas.restore();
    }
    // Diamond core.
    canvas.rotate(0.7853981);
    final d = 0.9 * unit;
    canvas.drawRect(
      Rect.fromCenter(center: Offset.zero, width: d, height: d),
      paint,
    );
    canvas.restore();

    // Vertical & horizontal beams.
    final beam = unit * 0.42;
    canvas.drawRect(
      Rect.fromLTWH(cx - beam / 2, cy - 3 * unit, beam, 8 * unit),
      paint,
    );
    canvas.drawRect(
      Rect.fromLTWH(cx - 3 * unit, cy - beam / 2, 6 * unit, beam),
      paint,
    );
  }

  @override
  bool shouldRepaint(GothicCrossPainter oldDelegate) =>
      oldDelegate.color != color;
}

/// Diamond slider thumb with a crimson glow (matches the web seek bar).
class DiamondThumb extends SliderComponentShape {
  const DiamondThumb();

  @override
  Size getPreferredSize(bool isEnabled, bool isDiscrete) =>
      const Size.square(18);

  @override
  void paint(
    PaintingContext context,
    Offset center, {
    required Animation<double> activationAnimation,
    required Animation<double> enableAnimation,
    required bool isDiscrete,
    required TextPainter labelPainter,
    required RenderBox parentBox,
    required SliderThemeData sliderTheme,
    required TextDirection textDirection,
    required double value,
    required double textScaleFactor,
    required Size sizeWithOverflow,
  }) {
    final canvas = context.canvas;
    final paint = Paint()..color = kCream;
    final shadow = Paint()
      ..color = kPink.withValues(alpha: 0.7)
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 8);
    final path = Path()
      ..moveTo(center.dx, center.dy - 9)
      ..lineTo(center.dx + 9, center.dy)
      ..lineTo(center.dx, center.dy + 9)
      ..lineTo(center.dx - 9, center.dy)
      ..close();
    canvas.drawPath(path, shadow);
    canvas.drawPath(path, paint);
  }
}
