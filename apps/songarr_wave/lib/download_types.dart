/// Where a download should be written. Resolved once per job (track or album)
/// so desktop only prompts for a folder a single time.
enum DownloadKind { mediaStore, directory, browser }

class DownloadDest {
  const DownloadDest(this.kind, [this.basePath]);
  final DownloadKind kind;

  /// Base directory for [DownloadKind.directory] (desktop folder / iOS docs).
  final String? basePath;
}
