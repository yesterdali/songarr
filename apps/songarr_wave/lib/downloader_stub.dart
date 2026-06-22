import 'package:web/web.dart' as web;

import 'download_types.dart';

/// Web: hand the URL to the browser, which downloads it (songarr sends
/// `Content-Disposition: attachment`, so it saves rather than navigates).
/// The browser controls the destination folder.
class PlatformDownloader {
  static bool get supported => true;

  static Future<DownloadDest?> pickDestination() async =>
      const DownloadDest(DownloadKind.browser);

  static Future<void> saveTo(
    DownloadDest dest, {
    required String url,
    required String baseName,
    String? subdir,
    required void Function(double) onProgress,
  }) async {
    final anchor = web.document.createElement('a') as web.HTMLAnchorElement;
    anchor.href = url;
    // Cross-origin downloads ignore this value and use the server's
    // Content-Disposition filename; harmless to set.
    anchor.download = '';
    anchor.style.display = 'none';
    web.document.body?.appendChild(anchor);
    anchor.click();
    anchor.remove();
    // The browser owns the transfer from here; we can't track its progress.
    onProgress(1);
  }
}
