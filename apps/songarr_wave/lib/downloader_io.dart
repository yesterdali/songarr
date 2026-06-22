import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:media_store_plus/media_store_plus.dart';
import 'package:path_provider/path_provider.dart';

import 'download_types.dart';

/// Saves original track bytes (no re-encode) to:
///   Android  → public Music/Songarr via MediaStore
///   iOS      → app Documents/Songarr (visible in the Files app)
///   Desktop  → a folder the user picks
class PlatformDownloader {
  static bool get supported => true;

  static final MediaStore _mediaStore = MediaStore();
  static bool _mediaInited = false;

  /// Resolve the destination once per job. Returns null if the user cancels.
  static Future<DownloadDest?> pickDestination() async {
    switch (defaultTargetPlatform) {
      case TargetPlatform.android:
        return const DownloadDest(DownloadKind.mediaStore);
      case TargetPlatform.iOS:
        final docs = await getApplicationDocumentsDirectory();
        return DownloadDest(DownloadKind.directory, '${docs.path}/Songarr');
      case TargetPlatform.linux:
      case TargetPlatform.windows:
      case TargetPlatform.macOS:
        final dir = await FilePicker.platform.getDirectoryPath(
          dialogTitle: 'Куда сохранить',
        );
        return dir == null ? null : DownloadDest(DownloadKind.directory, dir);
      default:
        return null;
    }
  }

  static Future<void> saveTo(
    DownloadDest dest, {
    required String url,
    required String baseName,
    String? subdir,
    required void Function(double) onProgress,
  }) async {
    final client = http.Client();
    try {
      final response = await client.send(http.Request('GET', Uri.parse(url)));
      if (response.statusCode < 200 || response.statusCode >= 300) {
        throw HttpException('HTTP ${response.statusCode}');
      }
      final fileName = '${_sanitize(baseName)}.${_extension(response.headers)}';

      if (dest.kind == DownloadKind.mediaStore) {
        final tmp = File('${(await getTemporaryDirectory()).path}/$fileName');
        await _pump(response, tmp, onProgress);
        await _publishToMediaStore(tmp.path, subdir);
        if (await tmp.exists()) await tmp.delete();
      } else {
        final dirPath = subdir == null
            ? dest.basePath!
            : '${dest.basePath!}/${_sanitize(subdir)}';
        await Directory(dirPath).create(recursive: true);
        await _pump(response, File('$dirPath/$fileName'), onProgress);
      }
      onProgress(1);
    } finally {
      client.close();
    }
  }

  static Future<void> _pump(
    http.StreamedResponse response,
    File file,
    void Function(double) onProgress,
  ) async {
    final total = response.contentLength ?? 0;
    var received = 0;
    final sink = file.openWrite();
    try {
      await for (final chunk in response.stream) {
        sink.add(chunk);
        received += chunk.length;
        if (total > 0) onProgress((received / total).clamp(0.0, 1.0));
      }
    } finally {
      await sink.close();
    }
  }

  static Future<void> _publishToMediaStore(String tempPath, String? subdir) async {
    if (!_mediaInited) {
      await MediaStore.ensureInitialized();
      MediaStore.appFolder = 'Songarr';
      _mediaInited = true;
    }
    final relativePath = subdir == null ? 'Songarr' : 'Songarr/${_sanitize(subdir)}';
    final info = await _mediaStore.saveFile(
      tempFilePath: tempPath,
      dirType: DirType.audio,
      dirName: DirName.music,
      relativePath: relativePath,
    );
    if (info == null) {
      throw const FileSystemException('MediaStore rejected the file');
    }
  }

  /// Extension from Content-Disposition filename, else Content-Type, else mp3.
  static String _extension(Map<String, String> headers) {
    final cd = headers['content-disposition'];
    if (cd != null) {
      final match = RegExp(
        r'filename\*?=(?:UTF-8'')?"?([^";]+)"?',
        caseSensitive: false,
      ).firstMatch(cd);
      if (match != null) {
        final name = Uri.decodeComponent(match.group(1)!.trim());
        final dot = name.lastIndexOf('.');
        if (dot > 0 && dot < name.length - 1) {
          return name.substring(dot + 1).toLowerCase();
        }
      }
    }
    final ct = (headers['content-type'] ?? '').split(';').first.trim().toLowerCase();
    switch (ct) {
      case 'audio/flac':
      case 'audio/x-flac':
        return 'flac';
      case 'audio/mpeg':
        return 'mp3';
      case 'audio/ogg':
      case 'audio/opus':
        return 'opus';
      case 'audio/mp4':
      case 'audio/m4a':
      case 'audio/x-m4a':
        return 'm4a';
      case 'audio/aac':
        return 'aac';
      case 'audio/wav':
      case 'audio/x-wav':
        return 'wav';
      default:
        return 'mp3';
    }
  }

  static String _sanitize(String name) {
    final cleaned = name
        .replaceAll(RegExp(r'[/\\:*?"<>|]'), '_')
        .replaceAll(RegExp(r'\s+'), ' ')
        .trim();
    return cleaned.length > 120 ? cleaned.substring(0, 120).trim() : cleaned;
  }
}
