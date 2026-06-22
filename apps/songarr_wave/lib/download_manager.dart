import 'package:flutter/foundation.dart';

import 'api.dart';
import 'downloader.dart';

enum DownloadState { downloading, done, error }

class DownloadTask {
  DownloadTask(this.label);
  final String label;
  DownloadState state = DownloadState.downloading;
  double progress = 0;
  String? error;
}

/// Tracks track/album downloads and their progress. Keyed by song id, and
/// `album:<id>` for whole-album jobs.
class DownloadManager extends ChangeNotifier {
  DownloadManager(this.api);

  final SongarrApi api;
  final Map<String, DownloadTask> _tasks = {};

  bool get supported => PlatformDownloader.supported;
  DownloadTask? taskFor(String key) => _tasks[key];
  static String albumKey(String albumId) => 'album:$albumId';

  Future<void> downloadTrack(Song song) async {
    if (_tasks[song.id]?.state == DownloadState.downloading) return;
    final dest = await PlatformDownloader.pickDestination();
    if (dest == null) return; // user cancelled the folder picker
    final task = DownloadTask(song.title);
    _set(song.id, task);
    try {
      await PlatformDownloader.saveTo(
        dest,
        url: api.downloadUrl(song),
        baseName: '${song.artist} - ${song.title}',
        onProgress: (p) {
          task.progress = p;
          notifyListeners();
        },
      );
      task
        ..state = DownloadState.done
        ..progress = 1;
    } catch (e) {
      task
        ..state = DownloadState.error
        ..error = e.toString();
    }
    notifyListeners();
  }

  Future<void> downloadAlbum(String albumId, String albumName, List<Song> songs) async {
    final key = albumKey(albumId);
    if (songs.isEmpty || _tasks[key]?.state == DownloadState.downloading) return;
    final dest = await PlatformDownloader.pickDestination();
    if (dest == null) return;
    final task = DownloadTask(albumName);
    _set(key, task);
    try {
      for (var i = 0; i < songs.length; i++) {
        await PlatformDownloader.saveTo(
          dest,
          url: api.downloadUrl(songs[i]),
          baseName: '${songs[i].artist} - ${songs[i].title}',
          subdir: albumName,
          onProgress: (p) {
            task.progress = (i + p) / songs.length;
            notifyListeners();
          },
        );
      }
      task
        ..state = DownloadState.done
        ..progress = 1;
    } catch (e) {
      task
        ..state = DownloadState.error
        ..error = e.toString();
    }
    notifyListeners();
  }

  void _set(String key, DownloadTask task) {
    _tasks[key] = task;
    notifyListeners();
  }
}
