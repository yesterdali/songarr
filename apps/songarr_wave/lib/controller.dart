import 'dart:async';

import 'package:flutter/material.dart';
import 'package:just_audio/just_audio.dart';

import 'api.dart';
import 'media_session.dart';

/// Holds the playback queue, the audio player, Wave state, and the system
/// media session. Every screen listens to it so auto-advance, likes, and
/// skips stay in sync (mirrors the React PlayerProvider).
class WaveController extends ChangeNotifier {
  WaveController(this.api) {
    _player = AudioPlayer();
    _media.bind(
      MediaTransport(
        onPlay: () => unawaited(_player.play()),
        onPause: () => unawaited(_player.pause()),
        onNext: () => unawaited(next()),
        onPrevious: () => unawaited(previous()),
        onSeek: (position) => unawaited(_player.seek(position)),
      ),
    );
    _playerSub = _player.playerStateStream.listen((state) {
      if (state.processingState == ProcessingState.completed &&
          !_autoAdvancing) {
        _autoAdvancing = true;
        unawaited(
          next(playAction: 'play').whenComplete(() {
            _autoAdvancing = false;
          }),
        );
      }
      _pushMediaState();
      notifyListeners();
    });
    _discontinuitySub = _player.positionDiscontinuityStream.listen((_) {
      _pushMediaState();
    });
    unawaited(_seedStarred());
    _prefetchWave();
  }

  final SongarrApi api;
  late final AudioPlayer _player;
  StreamSubscription<PlayerState>? _playerSub;
  StreamSubscription<PositionDiscontinuity>? _discontinuitySub;
  final MediaSession _media = MediaSession();

  Future<List<Song>>? _wavePrefetch;
  bool _waveReady = false;

  final List<Song> _queue = [];
  int _index = 0;
  Set<String> _starredIds = {};
  bool _extending = false;
  bool _autoAdvancing = false;
  int _loadToken = 0;
  bool loadingWave = false;
  String? error;

  AudioPlayer get player => _player;
  List<Song> get queue => List.unmodifiable(_queue);
  int get index => _index;
  Song? get current => _queue.isEmpty ? null : _queue[_index];
  bool get isPlaying => _player.playing;
  bool get waveReady => _waveReady;
  bool isStarred(String id) => _starredIds.contains(id);

  List<Song> upNext([int take = 8]) =>
      _queue.skip(_index + 1).take(take).toList(growable: false);

  Future<void> _seedStarred() async {
    try {
      _starredIds = await api.getStarredIds();
      notifyListeners();
    } catch (_) {
      // Best-effort; the heart simply starts empty.
    }
  }

  void _prefetchWave() {
    final prefetch = api.waveNext(count: 12);
    _wavePrefetch = prefetch;
    unawaited(
      prefetch
          .then<void>((songs) {
            if (songs.isNotEmpty) {
              _waveReady = true;
              if (_queue.isEmpty) _warmCover(songs.first);
              notifyListeners();
            }
          })
          .catchError((Object _) {
            _waveReady = false;
            notifyListeners();
          }),
    );
  }

  Future<void> startWave() async {
    if (loadingWave) return;
    loadingWave = true;
    error = null;
    notifyListeners();
    try {
      var songs = const <Song>[];
      final prefetch = _wavePrefetch;
      _wavePrefetch = null;
      if (prefetch != null) {
        try {
          songs = await prefetch;
        } catch (_) {
          // Fall through to a fresh fetch.
        }
      }
      if (songs.isEmpty) songs = await api.waveNext(count: 12);
      if (songs.isEmpty) {
        throw const SongarrException('Wave returned no tracks yet');
      }
      _waveReady = true;
      _queue
        ..clear()
        ..addAll(songs);
      _index = 0;
      await _playCurrent();
    } catch (e) {
      error = e.toString();
    } finally {
      loadingWave = false;
      notifyListeners();
    }
  }

  /// Replace the queue with [songs] and play from [startIndex] (used by
  /// albums, playlists, search results, liked songs).
  Future<void> playQueue(List<Song> songs, [int startIndex = 0]) async {
    if (songs.isEmpty) return;
    _queue
      ..clear()
      ..addAll(songs);
    _index = startIndex.clamp(0, songs.length - 1);
    error = null;
    notifyListeners();
    await _playCurrent();
  }

  Future<void> _playCurrent() async {
    final song = current;
    if (song == null) return;
    final token = ++_loadToken;
    _pushMediaMetadata();
    _pushMediaState();
    notifyListeners();
    try {
      // Reset the element before loading: on web, setUrl while a source is
      // still playing can otherwise keep playing the old one.
      await _player.stop();
      if (token != _loadToken) return;
      await _player.setUrl(api.streamUrl(song));
      if (token != _loadToken) return;
      await _player.play();
      if (token != _loadToken) return;
      error = null;
    } catch (e) {
      if (token != _loadToken) return;
      // Surface the failure instead of silently leaving the previous track
      // playing (which looks like the player being "stuck").
      error = e.toString();
      await _player.stop();
      notifyListeners();
      return;
    }
    _pushMediaMetadata();
    _pushMediaState();
    unawaited(api.feedback(song.id, 'play'));
    unawaited(_extendQueueIfNeeded());
    _warmLyrics();
    _warmNext();
    notifyListeners();
  }

  void _pushMediaMetadata() {
    final song = current;
    if (song == null) {
      _media.clear();
      return;
    }
    _media.setMetadata(
      NowPlayingInfo(
        title: song.title,
        artist: song.artist,
        album: song.album,
        artworkUrl: (song.coverArt == null || song.coverArt!.isEmpty)
            ? null
            : api.coverUrl(song.coverArt, size: 512),
      ),
    );
  }

  void _pushMediaState() {
    final song = current;
    final duration =
        _player.duration ??
        (song?.duration != null
            ? Duration(seconds: song!.duration!)
            : Duration.zero);
    _media
      ..setPlaybackState(playing: _player.playing)
      ..setPosition(
        position: _player.position,
        duration: duration,
        speed: _player.speed,
      );
  }

  void _warmLyrics() {
    final song = current;
    if (song != null) unawaited(api.getLyrics(song.id).catchError((_) => null));
    final next = _index + 1 < _queue.length ? _queue[_index + 1] : null;
    if (next != null) unawaited(api.getLyrics(next.id).catchError((_) => null));
  }

  // Warm only the next track's cover (no audio): a second AudioPlayer holds a
  // songarr stream slot open, which starves real playback at low
  // `max_concurrent` and leaves the previous track stuck.
  void _warmNext() {
    final next = _index + 1 < _queue.length ? _queue[_index + 1] : null;
    if (next != null) _warmCover(next);
  }

  void _warmCover(Song song) {
    final url = api.coverUrl(song.coverArt, size: 360);
    if (url.isEmpty) return;
    NetworkImage(url).resolve(const ImageConfiguration());
  }

  Future<void> _extendQueueIfNeeded() async {
    if (_extending || _queue.length - _index > 4) return;
    _extending = true;
    final seed = current;
    try {
      var more = await api.waveNext(count: 12, seedId: seed?.id);
      if (more.isEmpty) more = await api.waveNext(count: 12);
      final seen = _queue.map((song) => song.id).toSet();
      _queue.addAll(more.where((song) => seen.add(song.id)));
      notifyListeners();
    } catch (_) {
      // Existing queue keeps playing.
    } finally {
      _extending = false;
    }
  }

  Future<void> playAt(int newIndex) async {
    if (newIndex < 0 || newIndex >= _queue.length) return;
    _index = newIndex;
    error = null;
    notifyListeners();
    await _playCurrent();
  }

  Future<void> next({String playAction = 'skip'}) async {
    final old = current;
    if (old != null) unawaited(api.feedback(old.id, playAction));
    if (_index + 1 >= _queue.length) {
      await _extendQueueIfNeeded();
    }
    if (_index + 1 < _queue.length) {
      _index += 1;
      error = null;
      notifyListeners();
      await _playCurrent();
    } else {
      await _player.pause();
      notifyListeners();
    }
  }

  Future<void> previous() async {
    if (_player.position.inSeconds > 3) {
      await _player.seek(Duration.zero);
      return;
    }
    if (_index > 0) {
      _index -= 1;
      error = null;
      notifyListeners();
      await _playCurrent();
    }
  }

  Future<void> toggle() async {
    if (_player.playing) {
      await _player.pause();
    } else if (current == null) {
      await startWave();
    } else {
      await _player.play();
    }
    notifyListeners();
  }

  Future<void> toggleStar(String id) async {
    final wasStarred = _starredIds.contains(id);
    if (wasStarred) {
      _starredIds.remove(id);
    } else {
      _starredIds.add(id);
      unawaited(api.feedback(id, 'like'));
    }
    notifyListeners();
    try {
      await (wasStarred ? api.unstar(id) : api.star(id));
    } catch (_) {
      if (wasStarred) {
        _starredIds.add(id);
      } else {
        _starredIds.remove(id);
      }
      notifyListeners();
    }
  }

  Future<void> dislikeCurrent() async {
    final song = current;
    if (song == null) return;
    unawaited(api.feedback(song.id, 'dislike'));
    await next(playAction: 'skip');
  }

  @override
  void dispose() {
    _loadToken++;
    _playerSub?.cancel();
    _discontinuitySub?.cancel();
    _media.dispose();
    _player.dispose();
    super.dispose();
  }
}
