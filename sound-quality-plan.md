# Sound quality controls — plan

## Goal
Let each listener pick playback quality (data-saver ↔ lossless), plus a separate
quality for offline downloads. Per device, because quality depends on the
device/network.

**Decisions baked in:** include a **Lossless/Original** tier (best-effort, with an
automatic mp3 fallback for Safari/WebKit); a **separate Download-quality** knob;
quality stored **per device** (localStorage). Discord voice bitrate stays a
server/bot config (already added) and is out of scope here — it's capped by the
channel's bitrate, a different ceiling than app streaming.

## How quality is applied (two paths)
1. **Library tracks → Navidrome (passthrough).** Honors the Subsonic params
   **`format`** + **`maxBitRate`** and transcodes on the fly; `format=raw` returns
   the original file untouched (lossless if FLAC). The whole lever for these is the
   web's `streamUrl()` (currently hardcoded `format=mp3&maxBitRate=320`).
2. **Virtual tracks (YouTube/Yandex/VK) → the proxy.** songarr transcodes these
   itself (ffmpeg) to `streaming.format`/`bitrate_kbps`; `maxBitRate` is ignored
   today. Needs a small change to honor a requested format/bitrate. (True
   `raw`/lossless is N/A here — the source is already lossy; map Lossless → the
   configured high bitrate for virtual tracks.)

## Tiers (one mapping, used by both streaming and download)
| Tier | format | maxBitRate |
|---|---|---|
| Low (data saver) | mp3 | 96 |
| Normal | mp3 | 192 |
| High *(current default)* | mp3 | 320 |
| Lossless / Original | raw | 0 (original; mp3-320 fallback on decode error) |
| Auto *(streaming only)* | mp3 | by network: Wi-Fi→320, cellular/saveData→128 |

mp3 for all transcoded tiers (universal browser support). **opus/ogg breaks in
Safari/WebKit** (the recs bug), and **FLAC** is unreliable there too — so Lossless
is best-effort with a fallback.

## Implementation

### Web
- New `quality.ts`: `getStreamQuality()` / `setStreamQuality()` and
  `getDownloadQuality()` / `set…` backed by localStorage; a `qualityParams(tier)`
  → `{ format, maxBitRate }`, with `Auto` resolved via the Network Information API
  (`navigator.connection.effectiveType` / `saveData`).
- `streamUrl(session, id)` reads `getStreamQuality()` and builds `format` +
  `maxBitRate` (no call-site changes needed since it reads the setting itself).
- **Lossless fallback:** in the player's load-error path, if a `raw`/lossless
  source errors (Safari/codec), retry once with `format=mp3&maxBitRate=320` so it
  never lands on silence.
- **Settings screen:** two selectors — *Качество стрима* (Auto/Low/Normal/High/
  Lossless) and *Качество загрузок* (Low/Normal/High/Lossless). Downloads use the
  download tier when fetching the blob (in `downloads.tsx`).
- Optional: show the active bitrate in the now-playing screen.

### Proxy
- Virtual-stream handler (`src/proxy/stream.rs`): read `format` (mp3|opus|raw→treat
  as configured) and `maxBitRate` from the query and use them for the ffmpeg
  transcode instead of only `streaming.*`. Clamp `maxBitRate` to a sane max. This
  makes the quality setting actually affect YouTube/Yandex/VK tracks.

### Listen Together
Nothing extra — each listener already streams from songarr, so their own quality
setting applies per person.

## Phasing
- **Phase 1:** streaming-quality selector + `streamUrl` wiring + the proxy change
  so it applies to virtual tracks too. (This is the bulk of the value, small.)
- **Phase 2:** separate download-quality + the Lossless mp3-fallback polish + the
  now-playing bitrate readout.

## Verification
- Unit: `qualityParams` mapping; Auto resolution from a mocked `connection`.
- Web build + tests.
- Manual/curl: `/rest/stream?...&format=mp3&maxBitRate=96` on a library track
  returns a smaller stream than `=320`; a virtual (`sgr_`) track honors the
  requested bitrate after the proxy change; Lossless on FLAC plays in Chrome and
  falls back to mp3 in Safari.
