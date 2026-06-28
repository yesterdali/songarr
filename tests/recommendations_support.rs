pub fn mock_yandex_helper() -> String {
    let dir = std::env::temp_dir().join(format!("songarr-yandex-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("songarr-yandex");
    std::fs::write(
        &path,
        r#"#!/usr/bin/env python3
import json, sys
cmd = sys.argv[1]
payload = json.loads(sys.stdin.read() or "{}")
tracks = [
  {"trackId": "ya-wave-1", "artist": "Yandex Artist", "title": "Yandex Wave", "album": "Yandex Album", "durationMs": 177000, "artworkUrl": "https://img.example/ya1.jpg"},
  {"trackId": "ya-wave-2", "artist": "Yandex Artist", "title": "Second Wave", "album": "Yandex Album", "durationMs": 188000, "artworkUrl": "https://img.example/ya2.jpg"}
]
if cmd in ("wave", "search"):
    print(json.dumps(tracks[: int(payload.get("limit", 20))]))
elif cmd == "download":
    print(json.dumps({"url": "http://127.0.0.1:9/yandex-audio.ogg", "codec": "mp3", "bitrateKbps": 192}))
else:
    print("unsupported", file=sys.stderr)
    sys.exit(2)
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path.to_string_lossy().into_owned()
}
