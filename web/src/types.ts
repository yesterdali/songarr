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
  song: Song;
  /** Unix epoch seconds of the play. */
  updatedAt: number;
};
