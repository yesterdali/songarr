import 'package:flutter/material.dart';

import 'api.dart';
import 'desktop_audio.dart';
import 'shell.dart';
import 'theme.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  // Enable the libmpv backend on Linux/Windows; no-op elsewhere.
  initDesktopAudio();
  runApp(const SongarrWaveApp());
}

class SongarrWaveApp extends StatelessWidget {
  const SongarrWaveApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Songarr Wave',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(),
      // Ambient gothic glow behind every route; Scaffolds are transparent.
      builder: (context, child) =>
          AmbientBackground(child: child ?? const SizedBox.shrink()),
      home: const WaveRoot(),
    );
  }
}

class WaveRoot extends StatefulWidget {
  const WaveRoot({super.key});

  @override
  State<WaveRoot> createState() => _WaveRootState();
}

class _WaveRootState extends State<WaveRoot> {
  SongarrApi? _api;

  @override
  Widget build(BuildContext context) {
    final api = _api;
    if (api == null) {
      return LoginScreen(onLogin: (api) => setState(() => _api = api));
    }
    return WaveShell(api: api, onLogout: () => setState(() => _api = null));
  }
}

class LoginScreen extends StatefulWidget {
  const LoginScreen({required this.onLogin, super.key});

  final ValueChanged<SongarrApi> onLogin;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _server = TextEditingController(text: 'http://10.0.2.2:4534');
  final _username = TextEditingController(text: 'admin');
  final _password = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _server.dispose();
    _username.dispose();
    _password.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final session = WaveSession(
      serverUrl: normalizeServer(_server.text),
      username: _username.text.trim(),
      password: _password.text,
    );
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final api = SongarrApi(session);
      await api.ping();
      if (!mounted) return;
      widget.onLogin(api);
    } catch (error) {
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 440),
            child: ListView(
              shrinkWrap: true,
              padding: const EdgeInsets.all(24),
              children: [
                Center(
                  child: Container(
                    width: 64,
                    height: 64,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(18),
                      gradient: const LinearGradient(
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                        colors: [Color(0xffa4243b), kPink, kViolet],
                      ),
                      boxShadow: [
                        BoxShadow(
                          color: kPink.withValues(alpha: 0.3),
                          blurRadius: 24,
                          offset: const Offset(0, 10),
                        ),
                      ],
                    ),
                    child: const Icon(
                      Icons.graphic_eq_rounded,
                      size: 32,
                      color: kCream,
                    ),
                  ),
                ),
                const SizedBox(height: 18),
                Text(
                  'Твоя волна',
                  textAlign: TextAlign.center,
                  style: serif(fontSize: 38, fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 4),
                Text(
                  'Songarr Wave',
                  textAlign: TextAlign.center,
                  style: serif(
                    fontSize: 18,
                    fontWeight: FontWeight.w500,
                    color: kPink,
                    letterSpacing: 3,
                  ),
                ),
                const SizedBox(height: 34),
                _GothicField(controller: _server, label: 'Songarr URL'),
                const SizedBox(height: 14),
                _GothicField(controller: _username, label: 'Username'),
                const SizedBox(height: 14),
                _GothicField(
                  controller: _password,
                  label: 'Password',
                  obscureText: true,
                  onSubmitted: (_) => _submit(),
                ),
                if (_error != null) ...[
                  const SizedBox(height: 16),
                  DecoratedBox(
                    decoration: BoxDecoration(
                      color: kPink.withValues(alpha: 0.14),
                      borderRadius: BorderRadius.circular(14),
                      border: Border.all(color: kPink.withValues(alpha: 0.28)),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(14),
                      child: Text(
                        _error!,
                        style: const TextStyle(color: Colors.white70),
                      ),
                    ),
                  ),
                ],
                const SizedBox(height: 22),
                FilledButton(
                  onPressed: _busy ? null : _submit,
                  style: FilledButton.styleFrom(
                    backgroundColor: kPink,
                    foregroundColor: kCream,
                    padding: const EdgeInsets.symmetric(vertical: 16),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(18),
                    ),
                  ),
                  child: Text(_busy ? 'Connecting...' : 'Log in'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _GothicField extends StatelessWidget {
  const _GothicField({
    required this.controller,
    required this.label,
    this.obscureText = false,
    this.onSubmitted,
  });

  final TextEditingController controller;
  final String label;
  final bool obscureText;
  final ValueChanged<String>? onSubmitted;

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: controller,
      obscureText: obscureText,
      onSubmitted: onSubmitted,
      style: const TextStyle(fontWeight: FontWeight.w700),
      decoration: InputDecoration(
        labelText: label,
        filled: true,
        fillColor: Colors.white.withValues(alpha: 0.05),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(18),
          borderSide: BorderSide(color: Colors.white.withValues(alpha: 0.12)),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(18),
          borderSide: const BorderSide(color: kPink, width: 2),
        ),
      ),
    );
  }
}
