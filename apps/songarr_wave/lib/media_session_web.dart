// Browser MediaSession backend: drives the OS media UI / keyboard media keys
// on web (Chrome, Edge, Firefox, Safari) and wires their controls back to the
// player. Compiled only on web via the conditional import in media_session.dart.

import 'dart:js_interop';

import 'package:web/web.dart' as web;

import 'media_session.dart';

/// The action object passed to a `seekto` handler. `package:web` types the
/// handler as a bare `JSFunction`, so we model just the field we read.
extension type _ActionDetails._(JSObject _) implements JSObject {
  external double? get seekTime;
}

class MediaSessionImpl implements MediaSession {
  web.MediaSession get _session => web.window.navigator.mediaSession;

  @override
  void bind(MediaTransport transport) {
    _session.setActionHandler('play', (() => transport.onPlay()).toJS);
    _session.setActionHandler('pause', (() => transport.onPause()).toJS);
    _session.setActionHandler('nexttrack', (() => transport.onNext()).toJS);
    _session.setActionHandler(
      'previoustrack',
      (() => transport.onPrevious()).toJS,
    );
    _session.setActionHandler(
      'seekto',
      ((_ActionDetails details) {
        final seekTime = details.seekTime;
        if (seekTime != null) {
          transport.onSeek(Duration(milliseconds: (seekTime * 1000).round()));
        }
      }).toJS,
    );
  }

  @override
  void setMetadata(NowPlayingInfo info) {
    final artwork = <web.MediaImage>[];
    final art = info.artworkUrl;
    if (art != null && art.isNotEmpty) {
      artwork.add(
        web.MediaImage(src: art, sizes: '512x512', type: 'image/jpeg'),
      );
    }
    _session.metadata = web.MediaMetadata(
      web.MediaMetadataInit(
        title: info.title,
        artist: info.artist,
        album: info.album ?? '',
        artwork: artwork.toJS,
      ),
    );
  }

  @override
  void setPlaybackState({required bool playing}) {
    _session.playbackState = playing ? 'playing' : 'paused';
  }

  @override
  void setPosition({
    required Duration position,
    required Duration duration,
    required double speed,
  }) {
    if (duration <= Duration.zero) return;
    final clamped = position > duration ? duration : position;
    _session.setPositionState(
      web.MediaPositionState(
        duration: duration.inMilliseconds / 1000.0,
        position: clamped.inMilliseconds / 1000.0,
        playbackRate: speed <= 0 ? 1.0 : speed,
      ),
    );
  }

  @override
  void clear() {
    _session.metadata = null;
    _session.playbackState = 'none';
  }

  @override
  void dispose() => clear();
}
