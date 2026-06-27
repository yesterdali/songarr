// Subsonic client over songarr's /rest/*. All calls carry the session auth;
// responses are the standard { "subsonic-response": {...} } envelope. songarr
// injects virtual (sgr_/sga_) results into search/artist/album/playlist, so
// the same plain Subsonic calls surface external content for free.

import { apiUrl, authQuery, type WaveSession } from "./auth";
import type {
  Album,
  Artist,
  FriendActivity,
  ListenState,
  LyricsResult,
  Playlist,
  Profile,
  RemoteState,
  Song,
} from "./types";

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
  provider?: string;
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
    // Intentionally ignore any server-provided streamUrl: wave/recs ship a
    // RELATIVE, opus-format URL that breaks in the Tauri webview (resolves
    // against tauri://localhost) and in Safari/WebKit (no ogg/opus decode).
    // The player builds an absolute, mp3 URL from the id instead.
    starred: Boolean(raw.starred),
    provider: raw.provider,
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

// Memoized per song id: the player warms lyrics for the current/next track,
// and the lyrics panel then resolves from the same promise instantly.
const lyricsCache = new Map<string, Promise<LyricsResult | null>>();
const LYRICS_CACHE_MAX = 40;

export function getLyrics(
  session: WaveSession,
  songId: string,
): Promise<LyricsResult | null> {
  const cached = lyricsCache.get(songId);
  if (cached) return cached;
  const promise = fetchLyrics(session, songId).catch((error: unknown) => {
    // Failed lookups may be transient — let the next caller retry.
    lyricsCache.delete(songId);
    throw error;
  });
  if (lyricsCache.size >= LYRICS_CACHE_MAX) {
    const oldest = lyricsCache.keys().next().value;
    if (oldest !== undefined) lyricsCache.delete(oldest);
  }
  lyricsCache.set(songId, promise);
  return promise;
}

async function fetchLyrics(
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

// ---- Friend Activity ----

/** Tell the server what the user is now playing (fire-and-forget). */
export async function reportNowPlaying(session: WaveSession, song: Song): Promise<void> {
  const response = await fetch(
    apiUrl(session, `/wave/api/now-playing?${authQuery(session)}`),
    {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({
        id: song.id,
        title: song.title,
        artist: song.artist,
        album: song.album ?? null,
        coverArt: song.coverArt ?? null,
      }),
    },
  );
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}

type RawFriend = {
  username: string;
  displayName?: string;
  id: string;
  title?: string;
  artist?: string;
  album?: string;
  coverArt?: string;
  updatedAt?: number;
};

/** What everyone else on the instance is listening to. */
export async function getFriends(session: WaveSession): Promise<FriendActivity[]> {
  const response = await fetch(apiUrl(session, `/wave/api/friends?${authQuery(session)}`), {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const body = (await response.json()) as { friends?: RawFriend[] };
  return (body.friends ?? []).map((raw) => ({
    username: raw.username,
    displayName: raw.displayName,
    song: toSong(raw),
    updatedAt: raw.updatedAt ?? 0,
  }));
}

// ---- Personalization ----

/** Avatar image URL for a user (an <img> src; 404s if none → fall back to initial). */
export function avatarUrl(session: WaveSession, username: string): string {
  return apiUrl(
    session,
    `/wave/api/avatar?${authQuery(session)}&user=${encodeURIComponent(username)}`,
  );
}

export async function getProfile(session: WaveSession): Promise<Profile> {
  const response = await fetch(apiUrl(session, `/wave/api/profile?${authQuery(session)}`), {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const raw = (await response.json()) as { displayName?: string | null; hasAvatar?: boolean };
  return { displayName: raw.displayName ?? null, hasAvatar: Boolean(raw.hasAvatar) };
}

export async function setDisplayName(session: WaveSession, displayName: string): Promise<void> {
  const response = await fetch(apiUrl(session, `/wave/api/profile?${authQuery(session)}`), {
    method: "PUT",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ displayName }),
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}

export async function uploadAvatar(session: WaveSession, image: Blob): Promise<void> {
  const response = await fetch(apiUrl(session, `/wave/api/profile/avatar?${authQuery(session)}`), {
    method: "PUT",
    headers: { "Content-Type": image.type || "image/jpeg" },
    body: image,
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}

export async function removeAvatar(session: WaveSession): Promise<void> {
  const response = await fetch(apiUrl(session, `/wave/api/profile/avatar?${authQuery(session)}`), {
    method: "DELETE",
    headers: { Accept: "application/json" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}

// ---- Listen Together ----

type RawListenTrack = {
  id: string;
  title?: string;
  artist?: string;
  provider?: string;
  artistId?: string;
  album?: string;
  albumId?: string;
  coverArt?: string;
  durationMs?: number;
};

type RawListenState = {
  code: string;
  host: string;
  isHost?: boolean;
  members?: { username: string; displayName?: string }[];
  track?: RawListenTrack | null;
  queue?: RawListenTrack[];
  events?: { id?: number; username?: string; kind?: string; text?: string; atMs?: number }[];
  anchorPosMs?: number;
  anchorTsMs?: number;
  isPlaying?: boolean;
  rev?: number;
  gone?: boolean;
};

function listenTrackToSong(raw: RawListenTrack): Song {
  return toSong({
    id: raw.id,
    title: raw.title,
    artist: raw.artist,
    provider: raw.provider,
    artistId: raw.artistId,
    album: raw.album,
    albumId: raw.albumId,
    coverArt: raw.coverArt,
    durationSecs: raw.durationMs ? Math.round(raw.durationMs / 1000) : undefined,
  });
}

function listenStateFromRaw(raw: RawListenState): ListenState {
  return {
    code: raw.code,
    host: raw.host,
    isHost: Boolean(raw.isHost),
    members: (raw.members ?? []).map((m) => ({ username: m.username, displayName: m.displayName })),
    track: raw.track ? listenTrackToSong(raw.track) : null,
    queue: (raw.queue ?? []).map(listenTrackToSong),
    events: (raw.events ?? [])
      .map((event) => ({
        id: event.id ?? 0,
        username: event.username ?? "",
        kind: event.kind ?? "chat",
        text: event.text ?? "",
        atMs: event.atMs ?? 0,
      }))
      .filter((event) => event.username && event.text),
    anchorPosMs: raw.anchorPosMs ?? 0,
    anchorTsMs: raw.anchorTsMs ?? 0,
    isPlaying: Boolean(raw.isPlaying),
    rev: raw.rev ?? 0,
  };
}

/** Server clock (epoch ms) for offset estimation. */
export async function getServerTime(session: WaveSession): Promise<number> {
  const response = await fetch(apiUrl(session, `/wave/api/time?${authQuery(session)}`), {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const body = (await response.json()) as { serverMs?: number };
  return body.serverMs ?? Date.now();
}

export async function createListen(session: WaveSession): Promise<string> {
  const response = await fetch(apiUrl(session, `/wave/api/listen/create?${authQuery(session)}`), {
    method: "POST",
    headers: { Accept: "application/json" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const body = (await response.json()) as { code: string };
  return body.code;
}

export async function joinListen(session: WaveSession, code: string): Promise<ListenState> {
  const response = await fetch(apiUrl(session, `/wave/api/listen/join?${authQuery(session)}`), {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ code }),
  });
  if (!response.ok) throw new Error(response.status === 404 ? "Комната не найдена" : `HTTP ${response.status}`);
  return listenStateFromRaw((await response.json()) as RawListenState);
}

export async function leaveListen(session: WaveSession, code: string): Promise<void> {
  await fetch(apiUrl(session, `/wave/api/listen/leave?${authQuery(session)}`), {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ code }),
  }).catch(() => undefined);
}

export async function getListenState(
  session: WaveSession,
  opts: { code: string; since?: number; wait?: number; signal?: AbortSignal },
): Promise<ListenState | null> {
  const query = new URLSearchParams(authQuery(session));
  query.set("code", opts.code);
  if (opts.since) query.set("since", String(opts.since));
  if (opts.wait) query.set("wait", String(opts.wait));
  const response = await fetch(apiUrl(session, `/wave/api/listen/state?${query.toString()}`), {
    headers: { Accept: "application/json" },
    signal: opts.signal,
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const raw = (await response.json()) as RawListenState;
  if (raw.gone) return null;
  return listenStateFromRaw(raw);
}

export async function listenCommand(
  session: WaveSession,
  code: string,
  action: string,
  payload?: unknown,
): Promise<void> {
  const query = new URLSearchParams(authQuery(session));
  query.set("code", code);
  await fetch(apiUrl(session, `/wave/api/listen/command?${query.toString()}`), {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ action, payload: payload ?? null }),
  }).then((response) => {
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
  });
}

// ---- Remote control (drive the Discord bot) ----

/** Queue a remote command for the bot (connect/disconnect/play/pause/…). */
export async function remoteCommand(
  session: WaveSession,
  action: string,
  payload?: unknown,
): Promise<void> {
  const response = await fetch(
    apiUrl(session, `/wave/api/remote/command?${authQuery(session)}`),
    {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({ action, payload: payload ?? null }),
    },
  );
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}

type RawRemoteState = {
  connected?: boolean;
  busy?: boolean;
  trackId?: string;
  title?: string;
  artist?: string;
  album?: string;
  coverArt?: string;
  positionMs?: number;
  durationMs?: number;
  isPlaying?: boolean;
  queue?: RawSong[];
  updatedAt?: number;
  rev?: number;
};

/** The bot's reported playback (for the remote playbar). Pass `wait`+`since` to
 *  long-poll: the request blocks until the state advances past `since` (rev). */
export async function getRemoteState(
  session: WaveSession,
  opts: { wait?: number; since?: number; signal?: AbortSignal } = {},
): Promise<RemoteState> {
  const query = new URLSearchParams(authQuery(session));
  if (opts.wait) query.set("wait", String(opts.wait));
  if (opts.since) query.set("since", String(opts.since));
  const response = await fetch(apiUrl(session, `/wave/api/remote/state?${query.toString()}`), {
    headers: { Accept: "application/json" },
    signal: opts.signal,
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const raw = (await response.json()) as RawRemoteState;
  const song = raw.trackId
    ? toSong({
        id: raw.trackId,
        title: raw.title,
        artist: raw.artist,
        album: raw.album,
        coverArt: raw.coverArt,
      })
    : null;
  return {
    connected: Boolean(raw.connected),
    busy: Boolean(raw.busy),
    song,
    positionMs: raw.positionMs ?? undefined,
    durationMs: raw.durationMs ?? undefined,
    isPlaying: Boolean(raw.isPlaying),
    queue: (raw.queue ?? []).map(toSong),
    updatedAt: raw.updatedAt ?? 0,
    rev: raw.rev ?? 0,
    fetchedAt: Date.now(),
  };
}
