# Offline Mode, Localization, And Lyrics Polish Plan

## Summary

Implement shared web-client improvements for the browser PWA and the Tauri desktop app: clearer offline playback from downloaded music, English/German/Russian localization, and a smoother lyrics experience. Keep storage in IndexedDB for this pass and do not add native Tauri filesystem plugins.

## Current State

- IndexedDB downloads already exist and are used for offline/instant audio playback.
- The PWA currently caches the app shell only; `/rest/*`, `/wave/api/*`, and audio streams stay live.
- Lyrics lookup and prefetch already exist through Songarr/Navidrome/LRCLIB.
- Most visible UI strings are currently hardcoded in Russian.

## Key Changes

- Offline mode:
  - Keep IndexedDB as the shared PWA/Tauri offline store.
  - Store downloaded audio plus song metadata, saved time, and cached cover bytes when available.
  - Add online/offline detection using `navigator.onLine` plus `online`/`offline` events.
  - Add a Downloaded library view with grouped downloaded songs, remove controls, and shuffle playback.
  - When offline, the Wave button starts a shuffled queue from downloaded tracks instead of calling `/wave/api/next`.
  - Show clearer download states for tracks, albums, and playlists.

- Localization:
  - Add a typed i18n layer with `en`, `de`, and `ru`.
  - Detect browser language on first run, fallback to English, and persist user override in settings.
  - Translate app chrome/copy while keeping music metadata untouched.
  - Use locale-aware date/time formatting where the UI previously hardcoded Russian formatting.

- Lyrics polish:
  - Keep Navidrome lyrics first and Songarr/LRCLIB fallback second.
  - Improve loading, not-found, failed, retry, synced, and plain lyrics states.
  - Improve synced lyrics readability and allow tapping synced lines to seek.
  - Show localized lyrics status text and make the lyrics button reflect lookup state.

## Test Plan

- Add web unit tests for language detection/persistence/key coverage.
- Add web unit tests for download-store migration/read/write/remove with mocked IndexedDB.
- Add web unit tests for lyrics active-line and display-state helpers.
- Run `pnpm build` and `pnpm exec vitest run`.
- Manually verify PWA and Tauri desktop flows: downloaded playback offline, language switching, lyrics retry/seek, and app-shell caching behavior.

## Assumptions

- Offline v1 means strong downloaded-library mode, not automatic bulk Wave downloads.
- IndexedDB remains the cross-platform storage layer for both PWA and Tauri.
- No backend DB migration is required.
- The plan file lives at the repository root because it covers web, PWA, and Tauri behavior together.
