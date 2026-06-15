import 'dart:convert';

import 'package:http/http.dart' as http;

class WaveSession {
  const WaveSession({
    required this.serverUrl,
    required this.username,
    required this.password,
  });

  final String serverUrl;
  final String username;
  final String password;
}

class SongarrException implements Exception {
  const SongarrException(this.message);
  final String message;

  @override
  String toString() => message;
}

class Song {
  const Song({
    required this.id,
    required this.title,
    required this.artist,
    this.artistId,
    this.album,
    this.albumId,
    this.duration,
    this.coverArt,
    this.streamUrl,
    this.provider,
    this.starred = false,
  });

  final String id;
  final String title;
  final String artist;
  final String? artistId;
  final String? album;
  final String? albumId;
  final int? duration;
  final String? coverArt;
  final String? streamUrl;
  final String? provider;
  final bool starred;

  factory Song.fromJson(Map<String, dynamic> json) {
    return Song(
      id: json['id'] as String,
      title: (json['title'] as String?) ?? 'Unknown',
      artist: (json['artist'] as String?) ?? 'Unknown artist',
      artistId: json['artistId'] as String?,
      album: json['album'] as String?,
      albumId: json['albumId'] as String?,
      duration: asInt(json['duration'] ?? json['durationSecs']),
      coverArt: (json['coverArt'] as String?) ?? json['id'] as String?,
      streamUrl: json['streamUrl'] as String?,
      provider: json['provider'] as String?,
      starred: json['starred'] != null && json['starred'] != false,
    );
  }
}

class Album {
  const Album({
    required this.id,
    required this.name,
    required this.artist,
    this.artistId,
    this.coverArt,
    this.songCount,
    this.year,
  });

  final String id;
  final String name;
  final String artist;
  final String? artistId;
  final String? coverArt;
  final int? songCount;
  final int? year;

  Album copyWith({String? coverArt}) => Album(
    id: id,
    name: name,
    artist: artist,
    artistId: artistId,
    coverArt: coverArt ?? this.coverArt,
    songCount: songCount,
    year: year,
  );

  factory Album.fromJson(Map<String, dynamic> json) {
    return Album(
      id: json['id'] as String,
      name:
          (json['name'] as String?) ??
          (json['title'] as String?) ??
          'Unknown album',
      artist: (json['artist'] as String?) ?? 'Unknown artist',
      artistId: json['artistId'] as String?,
      coverArt: json['coverArt'] as String?,
      songCount: asInt(json['songCount']),
      year: asInt(json['year']),
    );
  }
}

class Artist {
  const Artist({
    required this.id,
    required this.name,
    this.coverArt,
    this.albumCount,
  });

  final String id;
  final String name;
  final String? coverArt;
  final int? albumCount;

  factory Artist.fromJson(Map<String, dynamic> json) {
    return Artist(
      id: json['id'] as String,
      name: (json['name'] as String?) ?? 'Unknown artist',
      coverArt: json['coverArt'] as String?,
      albumCount: asInt(json['albumCount']),
    );
  }
}

class Playlist {
  const Playlist({
    required this.id,
    required this.name,
    this.songCount,
    this.coverArt,
  });

  final String id;
  final String name;
  final int? songCount;
  final String? coverArt;

  factory Playlist.fromJson(Map<String, dynamic> json) {
    return Playlist(
      id: json['id'] as String,
      name: (json['name'] as String?) ?? 'Playlist',
      songCount: asInt(json['songCount']),
      coverArt: (json['coverArt'] as String?) ?? json['id'] as String?,
    );
  }
}

class LyricsLine {
  const LyricsLine({this.start, required this.value});

  /// Start offset in milliseconds, when the lyrics are time-synced.
  final int? start;
  final String value;
}

class LyricsResult {
  const LyricsResult({
    this.artist,
    this.title,
    required this.synced,
    required this.lines,
  });

  final String? artist;
  final String? title;
  final bool synced;
  final List<LyricsLine> lines;
}

class SearchResults {
  const SearchResults({
    required this.songs,
    required this.albums,
    required this.artists,
  });
  final List<Song> songs;
  final List<Album> albums;
  final List<Artist> artists;

  bool get isEmpty => songs.isEmpty && albums.isEmpty && artists.isEmpty;
}

class StarredAll {
  const StarredAll({
    required this.songs,
    required this.albums,
    required this.artists,
  });
  final List<Song> songs;
  final List<Album> albums;
  final List<Artist> artists;
}

class SongarrApi {
  SongarrApi(this.session);

  final WaveSession session;
  final http.Client _client = http.Client();

  Uri _uri(String path, [Map<String, String> params = const {}]) {
    final base = Uri.parse(session.serverUrl);
    final auth = <String, String>{
      'u': session.username,
      'p': session.password,
      'v': '1.16.1',
      'c': 'songarr_flutter',
      'f': 'json',
    };
    return base.replace(
      path: _joinPath(base.path, path),
      queryParameters: {...auth, ...params},
    );
  }

  String coverUrl(String? coverArt, {int size = 600}) {
    if (coverArt == null || coverArt.isEmpty) return '';
    return _uri('/rest/getCoverArt', {
      'id': coverArt,
      'size': '$size',
    }).toString();
  }

  String streamUrl(Song song) {
    if (song.streamUrl != null && song.streamUrl!.isNotEmpty) {
      return Uri.parse(session.serverUrl).resolve(song.streamUrl!).toString();
    }
    return _uri('/rest/stream', {
      'id': song.id,
      'format': 'mp3',
      'maxBitRate': '320',
    }).toString();
  }

  Future<void> ping() async {
    final body = await _getJson(_uri('/rest/ping'));
    _ensureOk(body);
  }

  Future<List<Song>> waveNext({int count = 12, String? seedId}) async {
    final params = {'count': '$count'};
    if (seedId != null) params['seedId'] = seedId;
    final body = await _getJson(_uri('/wave/api/next', params));
    final tracks = body['tracks'];
    if (tracks is! List) return const [];
    return tracks
        .whereType<Map<String, dynamic>>()
        .map(Song.fromJson)
        .toList(growable: false);
  }

  Future<void> feedback(String trackId, String action) async {
    final uri = _uri('/wave/api/feedback');
    await _client.post(
      uri,
      headers: {
        'Accept': 'application/json',
        'Content-Type': 'application/json',
      },
      body: jsonEncode({'trackId': trackId, 'action': action}),
    );
  }

  Future<SearchResults> search(String query) async {
    final body = await _subsonic('/rest/search3', {
      'query': query,
      'songCount': '30',
      'albumCount': '12',
      'artistCount': '12',
    });
    final result = body['searchResult3'];
    if (result is! Map<String, dynamic>) {
      return const SearchResults(songs: [], albums: [], artists: []);
    }
    return SearchResults(
      songs: _list(result['song']).map(Song.fromJson).toList(),
      albums: _list(result['album']).map(Album.fromJson).toList(),
      artists: _list(result['artist']).map(Artist.fromJson).toList(),
    );
  }

  Future<List<Artist>> getArtists() async {
    final body = await _subsonic('/rest/getArtists');
    final artists = body['artists'];
    if (artists is! Map<String, dynamic>) return const [];
    final index = artists['index'];
    final indices = index is List
        ? index
        : (index == null ? const [] : [index]);
    final out = <Artist>[];
    for (final entry in indices.whereType<Map<String, dynamic>>()) {
      out.addAll(_list(entry['artist']).map(Artist.fromJson));
    }
    return out;
  }

  Future<({Artist artist, List<Album> albums})> getArtist(String id) async {
    final body = await _subsonic('/rest/getArtist', {'id': id});
    final raw = body['artist'];
    final map = raw is Map<String, dynamic> ? raw : <String, dynamic>{'id': id};
    return (
      artist: Artist.fromJson(map),
      albums: _list(map['album']).map(Album.fromJson).toList(),
    );
  }

  Future<({Album album, List<Song> songs})> getAlbum(String id) async {
    final body = await _subsonic('/rest/getAlbum', {'id': id});
    final raw = body['album'];
    final map = raw is Map<String, dynamic> ? raw : <String, dynamic>{'id': id};
    final songs = _list(map['song']).map(Song.fromJson).toList();
    var album = Album.fromJson(map);
    if (album.coverArt == null || album.coverArt!.isEmpty) {
      final fromSong = songs
          .map((s) => s.coverArt)
          .firstWhere((c) => c != null && c.isNotEmpty, orElse: () => null);
      if (fromSong != null) album = album.copyWith(coverArt: fromSong);
    }
    return (album: album, songs: songs);
  }

  Future<List<Album>> getAlbumList(String type, {int size = 24}) async {
    final body = await _subsonic('/rest/getAlbumList2', {
      'type': type,
      'size': '$size',
    });
    final list = body['albumList2'];
    if (list is! Map<String, dynamic>) return const [];
    return _list(list['album']).map(Album.fromJson).toList();
  }

  /// Fill missing cover art for the first [limit] albums by fetching detail.
  Future<List<Album>> repairAlbumCovers(
    List<Album> albums, {
    int? limit,
  }) async {
    final cap = limit ?? albums.length;
    final out = <Album>[];
    for (var i = 0; i < albums.length; i++) {
      final album = albums[i];
      if (album.coverArt != null && album.coverArt!.isNotEmpty || i >= cap) {
        out.add(album);
        continue;
      }
      try {
        final detail = await getAlbum(album.id);
        out.add(album.copyWith(coverArt: detail.album.coverArt));
      } catch (_) {
        out.add(album);
      }
    }
    return out;
  }

  Future<List<Playlist>> getPlaylists() async {
    final body = await _subsonic('/rest/getPlaylists');
    final lists = body['playlists'];
    if (lists is! Map<String, dynamic>) return const [];
    return _list(lists['playlist']).map(Playlist.fromJson).toList();
  }

  Future<({Playlist playlist, List<Song> songs})> getPlaylist(String id) async {
    final body = await _subsonic('/rest/getPlaylist', {'id': id});
    final raw = body['playlist'];
    final map = raw is Map<String, dynamic> ? raw : <String, dynamic>{'id': id};
    return (
      playlist: Playlist.fromJson(map),
      songs: _list(map['entry']).map(Song.fromJson).toList(),
    );
  }

  Future<StarredAll> getStarred() async {
    final body = await _subsonic('/rest/getStarred2');
    final starred = body['starred2'];
    if (starred is! Map<String, dynamic>) {
      return const StarredAll(songs: [], albums: [], artists: []);
    }
    return StarredAll(
      songs: _list(starred['song']).map(Song.fromJson).toList(),
      albums: _list(starred['album']).map(Album.fromJson).toList(),
      artists: _list(starred['artist']).map(Artist.fromJson).toList(),
    );
  }

  Future<Set<String>> getStarredIds() async {
    final starred = await getStarred();
    return starred.songs.map((song) => song.id).toSet();
  }

  Future<void> star(String id) => _subsonic('/rest/star', {'id': id});

  Future<void> unstar(String id) => _subsonic('/rest/unstar', {'id': id});

  // Memoized per song id: the player warms lyrics for the current/next track,
  // so opening the lyrics panel resolves from the same future instantly.
  final Map<String, Future<LyricsResult?>> _lyricsCache = {};

  Future<LyricsResult?> getLyrics(String songId) {
    final cached = _lyricsCache[songId];
    if (cached != null) return cached;
    final future = _fetchLyrics(songId).catchError((Object error) {
      _lyricsCache.remove(songId);
      throw error;
    });
    if (_lyricsCache.length >= 40) {
      _lyricsCache.remove(_lyricsCache.keys.first);
    }
    _lyricsCache[songId] = future;
    return future;
  }

  Future<LyricsResult?> _fetchLyrics(String songId) async {
    final body = await _subsonic('/rest/getLyricsBySongId', {'id': songId});
    final list = body['lyricsList'];
    if (list is! Map<String, dynamic>) return null;
    final structured = list['structuredLyrics'];
    final Map<String, dynamic>? first;
    if (structured is List) {
      final maps = structured.whereType<Map<String, dynamic>>();
      first = maps.isEmpty ? null : maps.first;
    } else {
      first = structured is Map<String, dynamic> ? structured : null;
    }
    if (first == null) return null;
    final lines = <LyricsLine>[];
    for (final line in _list(first['line'])) {
      final value = (line['value'] as String?)?.trim() ?? '';
      if (value.isEmpty) continue;
      lines.add(LyricsLine(start: asInt(line['start']), value: value));
    }
    if (lines.isEmpty) return null;
    return LyricsResult(
      artist: first['displayArtist'] as String?,
      title: first['displayTitle'] as String?,
      synced: first['synced'] == true,
      lines: lines,
    );
  }

  List<Map<String, dynamic>> _list(Object? value) {
    if (value is List) return value.whereType<Map<String, dynamic>>().toList();
    if (value is Map<String, dynamic>) return [value];
    return const [];
  }

  Future<Map<String, dynamic>> _subsonic(
    String path, [
    Map<String, String> params = const {},
  ]) async {
    final body = await _getJson(_uri(path, params));
    _ensureOk(body);
    final subsonic = body['subsonic-response'];
    return subsonic is Map<String, dynamic> ? subsonic : <String, dynamic>{};
  }

  Future<Map<String, dynamic>> _getJson(Uri uri) async {
    final response = await _client.get(
      uri,
      headers: {'Accept': 'application/json'},
    );
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw SongarrException('HTTP ${response.statusCode}');
    }
    final decoded = jsonDecode(response.body);
    if (decoded is! Map<String, dynamic>) {
      throw const SongarrException('Invalid server response');
    }
    return decoded;
  }

  void _ensureOk(Map<String, dynamic> body) {
    final subsonic = body['subsonic-response'];
    if (subsonic is! Map<String, dynamic> || subsonic['status'] != 'ok') {
      final error = subsonic is Map<String, dynamic> ? subsonic['error'] : null;
      final message = error is Map<String, dynamic>
          ? error['message'] as String?
          : null;
      throw SongarrException(message ?? 'Login failed');
    }
  }
}

String _joinPath(String basePath, String path) {
  final prefix = basePath.endsWith('/')
      ? basePath.substring(0, basePath.length - 1)
      : basePath;
  final suffix = path.startsWith('/') ? path : '/$path';
  return '$prefix$suffix';
}

int? asInt(Object? value) {
  if (value is int) return value;
  if (value is num) return value.round();
  if (value is String) return int.tryParse(value);
  return null;
}

String normalizeServer(String value) {
  final trimmed = value.trim();
  if (trimmed.isEmpty) return 'http://10.0.2.2:4534';
  if (trimmed.contains('://')) return trimmed.replaceAll(RegExp(r'/+$'), '');
  return 'https://${trimmed.replaceAll(RegExp(r'/+$'), '')}';
}

String formatDuration(Duration duration) {
  final minutes = duration.inMinutes.remainder(60).toString();
  final seconds = duration.inSeconds.remainder(60).toString().padLeft(2, '0');
  final hours = duration.inHours;
  return hours > 0 ? '$hours:$minutes:$seconds' : '$minutes:$seconds';
}
