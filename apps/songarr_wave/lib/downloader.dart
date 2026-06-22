// Picks the file-saving backend at compile time: real native I/O + MediaStore
// on platforms with dart:io, a no-op on web (which can't write to disk).
export 'downloader_stub.dart' if (dart.library.io) 'downloader_io.dart';
