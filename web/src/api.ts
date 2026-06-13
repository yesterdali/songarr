// Subsonic client over songarr's /rest/*. All calls carry the session auth;
// responses are the standard { "subsonic-response": {...} } envelope. songarr
// injects virtual (sgr_/sga_) results into search/artist/album/playlist, so
// the same plain Subsonic calls surface external content for free.

import { apiUrl, authQuery, type WaveSession } from "./auth";
import type { Album, Artist, LyricsResult, Playlist, Song } from "./types";

const ARTIST_COVER_CACHE_VERSION = "songarr.wave.artistCovers.v1";

type Envelope = {
  "subsonic-response"?: {
    status?: string;
    error?: { message?: string };
    [key: string]: unknown;
  };
};

async function call(
  session: WaveSession,
  endpoint: string,
  params: Record<string, string | number | undefined> = {},
): Promise<Record<string, unknown>> {
  const query = new URLSearchParams(authQuery(session));
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== "") {
      query.set(key, String(value));
    }
  }
  const response = await fetch(apiUrl(session, `/rest/${endpoint}?${query.toString()}`), {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  const body = (await response.json()) as Envelope;
  const subsonic = body["subsonic-response"];
  if (!subsonic || subsonic.status !== "ok") {
    throw new Error(subsonic?.error?.message ?? "Request failed");
  }
  return subsonic;
}

function asArray<T>(value: unknown): T[] {
  if (Array.isArray(value)) return value as T[];
  if (value === undefined || value === null) return [];
  return [value as T];
}

function localStorageOrNull(): Storage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

function artistCoverCacheKey(session: WaveSession): string {
  return `${ARTIST_COVER_CACHE_VERSION}:${session.serverUrl}:${session.username}`;
}

function readArtistCoverCache(session: WaveSession): Record<string, string> {
  const storage = localStorageOrNull();
  if (!storage) return {};
  try {
    const parsed = JSON.parse(storage.getItem(artistCoverCacheKey(session)) ?? "{}");
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(
        (entry): entry is [string, string] =>
          typeof entry[0] === "string" && typeof entry[1] === "string",
      ),
    );
  } catch {
    return {};
  }
}

function writeArtistCoverCache(session: WaveSession, cache: Record<string, string>): void {
  const storage = localStorageOrNull();
  if (!storage) return;
  const entries = Object.entries(cache).slice(-1000);
  try {
    storage.setItem(artistCoverCacheKey(session), JSON.stringify(Object.fromEntries(entries)));
  } catch {
    // localStorage can be full or unavailable in private browsing; covers still work live.
  }
}

type RawSong = {
  id: string;
  title?: string;
  artist?: string;
  artistId?: string;
  album?: string;
  albumId?: string;
  duration?: number;
  durationSecs?: number;
  coverArt?: string;
  streamUrl?: string;
  starred?: string;
};

function toSong(raw: RawSong): Song {
  return {
    id: raw.id,
    title: raw.title ?? "Unknown",
    artist: raw.artist ?? "Unknown artist",
    artistId: raw.artistId,
    album: raw.album,
    albumId: raw.albumId,
    duration: raw.duration ?? raw.durationSecs,
    coverArt: raw.coverArt ?? raw.id,
    streamUrl: raw.streamUrl,
    starred: Boolean(raw.starred),
  };
}

type RawAlbum = {
  id: string;
  name?: string;
  title?: string;
  artist?: string;
  artistId?: string;
  coverArt?: string;
  songCount?: number;
  year?: number;
  starred?: string;
};

function toAlbum(raw: RawAlbum): Album {
  return {
    id: raw.id,
    name: raw.name ?? raw.title ?? "Unknown album",
    artist: raw.artist ?? "Unknown artist",
    artistId: raw.artistId,
    coverArt: raw.coverArt,
    songCount: raw.songCount,
    year: raw.year,
    starred: Boolean(raw.starred),
  };
}

type RawArtist = { id: string; name?: string; coverArt?: string; albumCount?: number };

function toArtist(raw: RawArtist): Artist {
  return {
    id: raw.id,
    name: raw.name ?? "Unknown artist",
    coverArt: raw.coverArt,
    albumCount: raw.albumCount,
  };
}

type RawPlaylist = {
  id: string;
  name?: string;
  songCount?: number;
  duration?: number;
  coverArt?: string;
  owner?: string;
  comment?: string;
};

function toPlaylist(raw: RawPlaylist): Playlist {
  return {
    id: raw.id,
    name: raw.name ?? "Playlist",
    songCount: raw.songCount,
    duration: raw.duration,
    coverArt: raw.coverArt ?? raw.id,
    owner: raw.owner,
    comment: raw.comment,
  };
}

type RawLyricsLine = {
  start?: number;
  value?: string;
};

type RawStructuredLyrics = {
  displayArtist?: string;
  displayTitle?: string;
  synced?: boolean;
  line?: unknown;
};

function toLyrics(raw: RawStructuredLyrics): LyricsResult {
  const lines = asArray<RawLyricsLine>(raw.line)
    .map((line) => ({ start: line.start, value: line.value ?? "" }))
    .filter((line) => line.value.trim() !== "");
  return {
    artist: raw.displayArtist,
    title: raw.displayTitle,
    synced: Boolean(raw.synced),
    lines,
  };
}

// ---- URLs the <audio> tag and <img> tags hit directly ----

export function streamUrl(session: WaveSession, id: string): string {
  return apiUrl(
    session,
    `/rest/stream?${authQuery(session)}&id=${encodeURIComponent(id)}&format=mp3&maxBitRate=320`,
  );
}

export function coverUrl(
  session: WaveSession,
  coverArt: string | undefined,
  size = 200,
): string | undefined {
  if (!coverArt) return undefined;
  return apiUrl(
    session,
    `/rest/getCoverArt?${authQuery(session)}&id=${encodeURIComponent(coverArt)}&size=${size}`,
  );
}

// ---- Endpoints ----

export async function search(
  session: WaveSession,
  query: string,
): Promise<{ songs: Song[]; albums: Album[]; artists: Artist[] }> {
  const result = (await call(session, "search3", {
    query,
    songCount: 30,
    albumCount: 12,
    artistCount: 12,
  }))["searchResult3"] as Record<string, unknown> | undefined;
  return {
    songs: asArray<RawSong>(result?.song).map(toSong),
    albums: asArray<RawAlbum>(result?.album).map(toAlbum),
    artists: asArray<RawArtist>(result?.artist).map(toArtist),
  };
}

export async function getArtists(session: WaveSession): Promise<Artist[]> {
  const artists = (await call(session, "getArtists"))["artists"] as
    | { index?: unknown }
    | undefined;
  return asArray<{ artist?: unknown }>(artists?.index)
    .flatMap((entry) => asArray<RawArtist>(entry.artist))
    .map(toArtist);
}

export async function getArtist(
  session: WaveSession,
  id: string,
): Promise<{ artist: Artist; albums: Album[] }> {
  const raw = (await call(session, "getArtist", { id }))["artist"] as
    | (RawArtist & { album?: unknown })
    | undefined;
  return {
    artist: toArtist(raw ?? { id }),
    albums: asArray<RawAlbum>(raw?.album).map(toAlbum),
  };
}

export async function repairArtistCovers(
  session: WaveSession,
  artists: Artist[],
  limit = artists.length,
): Promise<Artist[]> {
  const cache = readArtistCoverCache(session);
  let changed = false;
  let repairs = 0;
  const withCached = artists.map((artist) => {
    const cached = cache[artist.id];
    return artist.coverArt || !cached ? artist : { ...artist, coverArt: cached };
  });

  const repaired = await Promise.all(
    withCached.map(async (artist) => {
      if (artist.coverArt || repairs >= limit) return artist;
      repairs += 1;
      try {
        const detail = await getArtist(session, artist.id);
        const coverArt =
          detail.artist.coverArt ?? detail.albums.find((album) => album.coverArt)?.coverArt;
        if (!coverArt) return artist;
        cache[artist.id] = coverArt;
        changed = true;
        return { ...artist, coverArt };
      } catch {
        return artist;
      }
    }),
  );

  if (changed) writeArtistCoverCache(session, cache);
  return repaired;
}

export async function getAlbum(
  session: WaveSession,
  id: string,
): Promise<{ album: Album; songs: Song[] }> {
  const raw = (await call(session, "getAlbum", { id }))["album"] as
    | (RawAlbum & { song?: unknown })
    | undefined;
  const songs = asArray<RawSong>(raw?.song).map(toSong);
  const album = toAlbum(raw ?? { id });
  return {
    album: {
      ...album,
      coverArt: album.coverArt ?? songs.find((song) => song.coverArt)?.coverArt,
    },
    songs,
  };
}

export async function getAlbumList(
  session: WaveSession,
  type: "newest" | "frequent" | "recent" | "alphabeticalByName",
  size = 24,
): Promise<Album[]> {
  const list = (await call(session, "getAlbumList2", { type, size }))[
    "albumList2"
  ] as { album?: unknown } | undefined;
  return asArray<RawAlbum>(list?.album).map(toAlbum);
}

export async function repairAlbumCovers(
  session: WaveSession,
  albums: Album[],
  limit = albums.length,
): Promise<Album[]> {
  const repaired = await Promise.all(
    albums.map(async (album, index) => {
      if (album.coverArt || index >= limit) return album;
      try {
        const detail = await getAlbum(session, album.id);
        return {
          ...album,
          coverArt: detail.album.coverArt,
        };
      } catch {
        return album;
      }
    }),
  );
  return repaired;
}

export async function getPlaylists(session: WaveSession): Promise<Playlist[]> {
  const lists = (await call(session, "getPlaylists"))["playlists"] as
    | { playlist?: unknown }
    | undefined;
  return asArray<RawPlaylist>(lists?.playlist).map(toPlaylist);
}

export async function getPlaylist(
  session: WaveSession,
  id: string,
): Promise<{ playlist: Playlist; songs: Song[] }> {
  const raw = (await call(session, "getPlaylist", { id }))["playlist"] as
    | (RawPlaylist & { entry?: unknown })
    | undefined;
  return {
    playlist: toPlaylist(raw ?? { id }),
    songs: asArray<RawSong>(raw?.entry).map(toSong),
  };
}

export async function getLyrics(
  session: WaveSession,
  songId: string,
): Promise<LyricsResult | null> {
  const list = (await call(session, "getLyricsBySongId", { id: songId }))[
    "lyricsList"
  ] as { structuredLyrics?: unknown } | undefined;
  const first = asArray<RawStructuredLyrics>(list?.structuredLyrics)[0];
  if (!first) return null;
  const lyrics = toLyrics(first);
  return lyrics.lines.length > 0 ? lyrics : null;
}

export async function getStarred(
  session: WaveSession,
): Promise<{ songs: Song[]; albums: Album[]; artists: Artist[] }> {
  const starred = (await call(session, "getStarred2"))["starred2"] as
    | Record<string, unknown>
    | undefined;
  return {
    songs: asArray<RawSong>(starred?.song).map(toSong),
    albums: asArray<RawAlbum>(starred?.album).map(toAlbum),
    artists: asArray<RawArtist>(starred?.artist).map(toArtist),
  };
}

export async function star(session: WaveSession, id: string): Promise<void> {
  await call(session, "star", { id });
}

export async function unstar(session: WaveSession, id: string): Promise<void> {
  await call(session, "unstar", { id });
}

export async function getWaveNext(
  session: WaveSession,
  params: { count?: number; seedId?: string } = {},
): Promise<Song[]> {
  const query = new URLSearchParams(authQuery(session));
  if (params.count) query.set("count", String(params.count));
  if (params.seedId) query.set("seedId", params.seedId);
  const response = await fetch(apiUrl(session, `/wave/api/next?${query.toString()}`), {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  const body = (await response.json()) as { tracks?: RawSong[] };
  return (body.tracks ?? []).map(toSong);
}

export async function waveFeedback(
  session: WaveSession,
  trackId: string,
  action: "play" | "skip" | "like" | "dislike",
): Promise<void> {
  const response = await fetch(apiUrl(session, `/wave/api/feedback?${authQuery(session)}`), {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ trackId, action }),
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
}
