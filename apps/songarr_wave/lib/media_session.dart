// Cross-platform system media session. The concrete implementation is chosen
// at compile time via a conditional import: a real browser MediaSession on
// web, a no-op everywhere else for now. Native backends — just_audio_background
// on mobile, SMTC (Windows) / MPRIS (Linux) / MPNowPlayingInfoCenter (macOS) on
// desktop — slot in behind this same interface without touching the controller.

import 'media_session_stub.dart'
    if (dart.library.js_interop) 'media_session_web.dart';

/// Metadata shown on the OS / browser media UI.
class NowPlayingInfo {
  const NowPlayingInfo({
    required this.title,
    required this.artist,
    this.album,
    this.artworkUrl,
  });

  final String title;
  final String artist;
  final String? album;
  final String? artworkUrl;
}

/// Transport callbacks the system media UI can invoke, wired back to the
/// playback controller.
class MediaTransport {
  const MediaTransport({
    required this.onPlay,
    required this.onPause,
    required this.onNext,
    required this.onPrevious,
    required this.onSeek,
  });

  final void Function() onPlay;
  final void Function() onPause;
  final void Function() onNext;
  final void Function() onPrevious;
  final void Function(Duration position) onSeek;
}

abstract class MediaSession {
  factory MediaSession() = MediaSessionImpl;

  /// Register the transport handlers once.
  void bind(MediaTransport transport);

  void setMetadata(NowPlayingInfo info);
  void setPlaybackState({required bool playing});
  void setPosition({
    required Duration position,
    required Duration duration,
    required double speed,
  });

  /// Clear all "now playing" state (queue drained / logged out).
  void clear();
  void dispose();
}
