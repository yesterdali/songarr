// No-op media session for platforms without a backend yet (mobile + desktop).
// Replaced per-platform later; the app runs unchanged in the meantime.

import 'media_session.dart';

class MediaSessionImpl implements MediaSession {
  @override
  void bind(MediaTransport transport) {}

  @override
  void setMetadata(NowPlayingInfo info) {}

  @override
  void setPlaybackState({required bool playing}) {}

  @override
  void setPosition({
    required Duration position,
    required Duration duration,
    required double speed,
  }) {}

  @override
  void clear() {}

  @override
  void dispose() {}
}
