// Subsonic entities, trimmed to the fields the Wave UI actually uses.

export type Song = {
  id: string;
  title: string;
  artist: string;
  artistId?: string;
  album?: string;
  albumId?: string;
  duration?: number; // seconds
  coverArt?: string;
  streamUrl?: string;
  starred?: boolean;
  provider?: string;
};

export type Album = {
  id: string;
  name: string;
  artist: string;
  artistId?: string;
  coverArt?: string;
  songCount?: number;
  year?: number;
  starred?: boolean;
};

export type Artist = {
  id: string;
  name: string;
  coverArt?: string;
  albumCount?: number;
};

export type Playlist = {
  id: string;
  name: string;
  songCount?: number;
  duration?: number;
  coverArt?: string;
  owner?: string;
  comment?: string;
};

export type LyricsLine = {
  start?: number; // milliseconds
  value: string;
};

export type LyricsResult = {
  artist?: string;
  title?: string;
  synced: boolean;
  lines: LyricsLine[];
};

// A friend's most-recent play (Spotify-style Friend Activity).
export type FriendActivity = {
  username: string;
  displayName?: string;
  song: Song;
  /** Unix epoch seconds of the play. */
  updatedAt: number;
};

// Personalization.
export type Profile = {
  displayName: string | null;
  hasAvatar: boolean;
};

// The Discord bot's reported playback, for remote control.
export type RemoteState = {
  connected: boolean;
  /** Bot is in use in another voice channel of this server. */
  busy: boolean;
  song: Song | null;
  positionMs?: number;
  durationMs?: number;
  isPlaying: boolean;
  queue: Song[];
  /** Server epoch seconds. */
  updatedAt: number;
  /** Monotonic revision — pass back as `since` to long-poll for the next change. */
  rev: number;
  /** Client Date.now() ms when received — used to interpolate position. */
  fetchedAt: number;
};
