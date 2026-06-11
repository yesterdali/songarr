# Songarr Artist Expansion — Implementation Plan (v1)

Companion to `songarr-proxy-plan.md` and `songarr-recs-plan.md`.

The goal is to make artist pages feel less like "only what I already
downloaded" and more like a discovery surface: when a user opens an artist in
Feishin, Supersonic, Symfonium, Amperfy, etc., Songarr should show local
library content first, then external catalog content that is playable through
the existing virtual-track machinery.

## 1. What we are building

**Artist Expansion** adds external catalog content to standard Subsonic artist
endpoints while preserving Navidrome as the source of truth for the local
library.

Today:

```text
Artist page -> Navidrome local albums/tracks only
```

Target:

```text
Artist page
  Local albums/tracks from Navidrome
  Songarr External albums/singles/top tracks from Deezer/YTM/etc.
    -> playable sgr_... virtual tracks
    -> materialize into the library on play, same as search/recs
```

The first useful version should not try to clone Spotify's full artist page.
It should make external artist catalogs visible in ordinary Subsonic clients
using endpoint shapes they already understand.

## 2. Product behavior

When a user opens an artist page:

- Local Navidrome albums and songs remain first.
- Songarr appends external content for that artist.
- External tracks are virtual `sgr_...` tracks and play/import exactly like
  search and recommendation results.
- External albums are virtual grouping objects only; they do not become real
  Navidrome albums until their tracks are imported.
- If external providers fail, the artist page falls back to vanilla Navidrome.

Expected client-facing result:

```text
Artist: Oxxxymiron

Albums
  [local albums]
  [Songarr] Горгород
  [Songarr] Красота и Уродство
  [Songarr] Singles & Top Tracks

Tracks
  [local tracks]
  sgr_... Где нас нет
  sgr_... Кто убил Марка?
  sgr_... Переплетено
```

Depending on client behavior, the most reliable surface may be either:

- injected virtual albums under `getArtist` / `getArtistInfo*`, or
- a synthetic "Songarr: Artist - NAME" playlist, or
- both.

## 3. Endpoint surface

Start with the endpoints most clients actually call for artist pages.

### Required for v1

- `getArtist`
  - For a real Navidrome artist id, pass through to Navidrome, then append
    virtual album summaries for external releases.
  - Preserve the upstream envelope and local content.

- `getAlbum`
  - For a virtual album id, synthesize an album response containing virtual
    `song` entries.
  - For a real album id, pass through unchanged.

- `getCoverArt`
  - Virtual album cover ids should fetch/cache provider artwork, reusing the
    existing virtual cover cache approach where possible.

- `stream` / `download`
  - No new behavior required: virtual album tracks are still normal `sgr_...`
    tracks.

### Strongly recommended

- `search3`
  - Optionally recognize artist-intent searches and surface virtual albums or
    artist top tracks.
  - Keep current song injection as-is.

- `getTopSongs`
  - Already implemented in R1/R2. Artist pages can reuse this provider logic
    for a "Singles & Top Tracks" virtual album.

### Optional / client dependent

- `getArtistInfo` / `getArtistInfo2`
  - Could append biography, similar artists, and external links later.
  - Not needed for first playable artist expansion.

- `getAlbumInfo` / `getAlbumInfo2`
  - Could synthesize provider album metadata.

## 4. Virtual id model

Existing track ids:

```text
sgr_<22 base62 uuid>
```

Artist expansion needs virtual album ids. Proposed scheme:

```text
sga_<22 base62 uuid>
```

Where:

- `sgr_` = Songarr virtual track
- `sga_` = Songarr virtual album

Do not introduce virtual artist ids in v1. Use real Navidrome artist ids as
the seed and resolve the artist name from Navidrome via admin calls.

Virtual album ids must be stable per provider album:

```text
UNIQUE(provider, provider_album_id)
```

For synthetic top-track groupings with no real provider album:

```text
provider = "songarr"
provider_album_id = "top:<normalized artist>"
```

## 5. Data model

Add a migration after `0003_recommendations.sql`.

```sql
CREATE TABLE virtual_albums (
  id TEXT PRIMARY KEY,              -- sga_...
  provider TEXT NOT NULL,           -- deezer | ytmusic | songarr
  provider_album_id TEXT NOT NULL,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  album_type TEXT,                  -- album | single | ep | compilation | top_tracks
  release_date TEXT,
  artwork_url TEXT,
  track_count INTEGER,
  status TEXT NOT NULL DEFAULT 'virtual',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(provider, provider_album_id)
);

CREATE TABLE virtual_album_tracks (
  virtual_album_id TEXT NOT NULL REFERENCES virtual_albums(id),
  virtual_track_id TEXT NOT NULL REFERENCES virtual_tracks(id),
  disc_number INTEGER,
  track_number INTEGER,
  PRIMARY KEY (virtual_album_id, virtual_track_id)
);

CREATE TABLE artist_catalog_cache (
  provider TEXT NOT NULL,
  artist_key TEXT NOT NULL,         -- normalized artist name
  payload_json TEXT NOT NULL,
  fetched_at_epoch INTEGER NOT NULL,
  PRIMARY KEY (provider, artist_key)
);
```

Potential simplification for first pass:

- Skip `virtual_album_tracks`.
- Store album track payloads in `virtual_albums.payload_json`.
- Normalize later if this becomes painful.

Prefer normalized tables if time allows; the query patterns are simple and
tests will be clearer.

## 6. Providers

### Deezer first

Deezer is the best first provider because:

- no API key,
- clean artist/album/track metadata,
- artwork URLs,
- stable album and track ids,
- already partially integrated.

Useful Deezer endpoints:

- Search artist by name.
- Artist albums.
- Album tracks.
- Artist top tracks.

Implementation shape:

```text
artist name
  -> Deezer artist search
  -> best artist match
  -> artist/{id}/albums
  -> album/{id}
  -> upsert virtual album
  -> upsert each track as virtual_tracks(provider='deezer')
  -> link tracks to album
```

### YTM later

YTM is useful for breadth and RU/new music, but album metadata is less stable
and parsing is more fragile. Use it as a fallback after Deezer is working.

### Last.fm / ListenBrainz

Not album providers. Useful later for similar artists and popularity hints,
not for first artist catalog expansion.

## 7. Matching and dedup

The artist page must not duplicate local library content.

Dedup rules:

- Local albums win over virtual albums when normalized artist+album matches.
- Local tracks win over virtual tracks when normalized artist+title matches,
  with duration tolerance when both durations are known.
- Imported `virtual_tracks.status = 'imported'` should also suppress matching
  virtual tracks.

Reuse the existing `SongKey` normalization from `proxy::search` where possible.
For album matching, add:

```rust
AlbumKey { artist, album }
```

Normalize with:

- lowercase,
- `deunicode`,
- ASCII alphanumeric only,
- later: RU transliteration hardening if needed.

## 8. Response synthesis

### `getArtist`

For JSON:

```json
{
  "subsonic-response": {
    "artist": {
      "id": "real_artist_id",
      "name": "Artist",
      "album": [
        { "id": "real_album", "name": "Local Album", ... },
        { "id": "sga_...", "name": "External Album", "artist": "Artist", ... }
      ]
    }
  }
}
```

For XML:

```xml
<artist id="real_artist_id" name="Artist">
  <album id="real_album" name="Local Album" ... />
  <album id="sga_..." name="External Album" artist="Artist" ... />
</artist>
```

Preserve upstream fields and ordering; append virtual albums after local ones.

### `getAlbum`

For a virtual album id:

```json
{
  "subsonic-response": {
    "album": {
      "id": "sga_...",
      "name": "External Album",
      "artist": "Artist",
      "song": [
        { "id": "sgr_...", "title": "Track", ... }
      ]
    }
  }
}
```

Use `SongEntry::from_virtual` for track entries, then add album-specific
fields if needed:

- `album`
- `track`
- `discNumber`
- `year` if known
- `coverArt`

## 9. Cache and refresh

Artist catalogs should be cached because artist pages can be opened often.

Recommended defaults:

```toml
[artist_expansion]
enabled = true
provider = "deezer"
max_albums = 12
max_tracks_per_album = 30
cache_ttl_hours = 168       # one week
include_singles = true
include_top_tracks_album = true
```

Cache miss:

- Fetch provider artist catalog.
- Upsert virtual albums/tracks.
- Return injected response.

Provider failure:

- Return vanilla Navidrome response.
- Log warning.
- Do not poison cache unless explicitly storing negative cache entries later.

## 10. Milestones

### A1 — Schema and provider catalog

- Add config section `[artist_expansion]`.
- Add `virtual_albums`, `virtual_album_tracks`, `artist_catalog_cache`.
- Add `catalog::deezer` artist/album methods.
- Unit tests for Deezer response parsing using fixtures.

Exit:

- Given a mocked Deezer artist, Songarr can upsert virtual albums and tracks.

### A2 — `getAlbum` for virtual albums

- Implement `proxy::album`.
- Real album ids pass through unchanged.
- `sga_...` ids synthesize JSON and XML album responses.
- Track entries are playable `sgr_...` songs.

Exit:

- `curl /rest/getAlbum?id=sga_...` returns a valid album with virtual tracks.
- `stream?id=<track from album>` works through existing M3 path.

### A3 — `getArtist` album injection

- Implement `proxy::artist`.
- Resolve real artist id -> artist name from Navidrome response.
- Fetch cached external catalog.
- Dedup local albums/tracks.
- Append virtual albums in JSON and XML.

Exit:

- Opening an artist page in Feishin shows local albums plus Songarr external
  albums.

### A4 — Top Tracks virtual album

- Add a synthetic album:

```text
Songarr: Top Tracks
```

- Populate it using R2 `getTopSongs` ensemble logic.
- This gives useful content even when album metadata is sparse.

Exit:

- Every expanded artist has at least one useful virtual album when providers
  return any top tracks.

### A5 — Real client verification

Test at least:

- Feishin desktop
- Supersonic desktop
- Amperfy iOS/macOS
- Symfonium Android

Record:

- Whether virtual albums show.
- Whether virtual album tracks play.
- Whether clients cache album contents aggressively.
- Whether `coverArt` works.
- Whether seeking works after import/remap.

## 11. Testing strategy

No tests should hit real external providers.

Add:

- `tests/artist_expansion.rs`
- mock Navidrome artist/album endpoints
- mock Deezer artist/album API
- JSON and XML coverage

Required tests:

- `getArtist` appends virtual albums after local albums.
- local album dedup suppresses matching virtual album.
- provider failure returns vanilla Navidrome body.
- virtual album id stability across repeated artist opens.
- `getAlbum` for virtual album returns virtual tracks.
- unknown `sga_...` returns Subsonic error 70.
- virtual album cover art fetches/caches artwork.
- track from virtual album streams via existing `stream` handler.

## 12. Definition of done

- Feishin artist page shows external albums/top tracks for an artist with a
  small local footprint.
- At least one external album can be opened and played end to end.
- Imported tracks no longer appear as duplicate virtual tracks on refresh.
- Provider outage leaves artist pages usable with local Navidrome content.
- JSON and XML tests pass.
- `cargo test` passes; Docker-harness suites continue to pass when enabled.

## 13. Open questions

- Should virtual albums be shown as real albums, or grouped under one
  synthetic "Songarr External" album to reduce clutter?
- Should singles be included by default? They improve discovery but can flood
  artist pages.
- Should artist expansion use only Deezer albums first, or immediately add a
  "Top Tracks" virtual album from the R2 ensemble?
- Do clients cache artist pages long enough that we need a cache-busting
  strategy for virtual album ids or `changed` timestamps?
- Should played tracks from virtual albums be appended to `Songarr Discovery`
  seeds immediately, or only after scrobble/import?
