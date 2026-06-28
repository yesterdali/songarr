import { createContext, useContext } from "react";
import type { WaveSession } from "./auth";
import type { ListenEvent, ListenMember, Song } from "./types";

export type RepeatMode = "off" | "all" | "one";

export type PlayerValue = {
  session: WaveSession;
  queue: Song[];
  index: number;
  current: Song | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  isWave: boolean;
  repeat: RepeatMode;
  shuffle: boolean;
  volume: number;
  muted: boolean;
  /** Remote control: true while the app is driving the Discord bot. */
  remoteOn: boolean;
  /** True once the bot has actually joined voice (fresh heartbeat). */
  remoteConnected: boolean;
  /** Bot is busy in another voice channel of the server. */
  remoteBusy: boolean;
  connectRemote: () => void;
  disconnectRemote: () => void;
  /** Listen Together: the room code if in one, else null. */
  listenCode: string | null;
  listenMembers: ListenMember[];
  listenEvents: ListenEvent[];
  startListen: () => Promise<string>;
  joinListen: (code: string) => Promise<void>;
  leaveListen: () => void;
  sendListenReaction: (emoji: string) => void;
  sendListenChat: (text: string) => void;
  playQueue: (songs: Song[], startIndex?: number) => void;
  startWave: () => Promise<void>;
  toggle: () => void;
  next: () => void;
  prev: () => void;
  seek: (seconds: number) => void;
  cycleRepeat: () => void;
  toggleShuffle: () => void;
  moreLikeCurrent: () => Promise<void>;
  setVolume: (value: number) => void;
  toggleMute: () => void;
  isStarred: (id: string) => boolean;
  toggleStar: (id: string) => void;
  dislikeCurrent: () => void;
  cover: (coverArt: string | undefined, size?: number) => string | undefined;
};

export const PlayerContext = createContext<PlayerValue | null>(null);

export function usePlayer(): PlayerValue {
  const value = useContext(PlayerContext);
  if (!value) throw new Error("usePlayer used outside PlayerProvider");
  return value;
}
