import 'package:flutter/foundation.dart';
import 'package:just_audio_media_kit/just_audio_media_kit.dart';

/// Route just_audio through libmpv (media_kit) on Linux & Windows, which have
/// no native just_audio backend. Android/iOS/macOS keep their efficient native
/// backends (ExoPlayer / AVPlayer), so we deliberately don't enable them here.
void initDesktopAudio() {
  if (defaultTargetPlatform == TargetPlatform.linux ||
      defaultTargetPlatform == TargetPlatform.windows) {
    JustAudioMediaKit.ensureInitialized(
      linux: true,
      windows: true,
      macOS: false,
      android: false,
      iOS: false,
    );
  }
}
