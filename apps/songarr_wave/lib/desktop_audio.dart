// Picks the right audio init at compile time: a real media_kit (libmpv) setup
// on native platforms, a no-op on web (which has no dart:io / media_kit).
export 'desktop_audio_stub.dart'
    if (dart.library.io) 'desktop_audio_io.dart';
