// Songarr Wave desktop: a thin Tauri shell that loads the bundled React web
// client (web/dist) in a native window. All app logic — auth, playback,
// offline downloads (IndexedDB) — runs in the webview exactly as on the web.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Songarr Wave");
}
