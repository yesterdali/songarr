// Playback engine: a single <audio>, a queue, and Media Session wiring, shared
// by every screen (and, later, the Wave button). Finite queue for Phase 2;
// Wave's endless auto-extend hooks the same `playQueue`/`next` in Phase 1.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  coverUrl,
  getStarred,
  getWaveNext,
  star,
  streamUrl,
  unstar,
  waveFeedback,
} from "./api";
import type { WaveSession } from "./auth";
import type { Song } from "./types";

type PlayerValue = {
  session: WaveSession;
  queue: Song[];
  index: number;
  current: Song | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  isWave: boolean;
  playQueue: (songs: Song[], startIndex?: number) => void;
  startWave: () => Promise<void>;
  toggle: () => void;
  next: () => void;
  prev: () => void;
  seek: (seconds: number) => void;
  isStarred: (id: string) => boolean;
  toggleStar: (id: string) => void;
  dislikeCurrent: () => void;
  cover: (coverArt: string | undefined, size?: number) => string | undefined;
};

const PlayerContext = createContext<PlayerValue | null>(null);

export function usePlayer(): PlayerValue {
  const value = useContext(PlayerContext);
  if (!value) throw new Error("usePlayer used outside PlayerProvider");
  return value;
}

export function PlayerProvider({
  session,
  children,
}: {
  session: WaveSession;
  children: ReactNode;
}) {
  const audioRef = useRef<HTMLAudioElement | null>(null);

  const [queue, setQueue] = useState<Song[]>([]);
  const [index, setIndex] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [starredIds, setStarredIds] = useState<Set<string>>(new Set());
  const [isWave, setIsWave] = useState(false);
  const extendingRef = useRef(false);

  const current = queue[index] ?? null;

  const cover = useCallback(
    (coverArt: string | undefined, size = 200) => coverUrl(session, coverArt, size),
    [session],
  );

  const advance = useCallback(
    (action: "play" | "skip") => {
      const track = queue[index];
      if (isWave && track) {
        waveFeedback(session, track.id, action).catch(() => undefined);
      }
      setIndex((i) => (i + 1 < queue.length ? i + 1 : i));
    },
    [index, isWave, queue, session],
  );

  const next = useCallback(() => {
    advance("skip");
  }, [advance]);

  const completeAndNext = useCallback(() => {
    advance("play");
  }, [advance]);

  const prev = useCallback(() => {
    const audio = audioRef.current;
    if (audio && audio.currentTime > 3) {
      audio.currentTime = 0;
      return;
    }
    setIndex((i) => (i > 0 ? i - 1 : i));
  }, []);

  // Wire audio element events once.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const onTime = () => setCurrentTime(audio.currentTime);
    const onDuration = () => setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
    const onPlay = () => setIsPlaying(true);
    const onPause = () => setIsPlaying(false);
    const onEnded = () => completeAndNext();
    audio.addEventListener("timeupdate", onTime);
    audio.addEventListener("durationchange", onDuration);
    audio.addEventListener("loadedmetadata", onDuration);
    audio.addEventListener("play", onPlay);
    audio.addEventListener("pause", onPause);
    audio.addEventListener("ended", onEnded);
    return () => {
      audio.removeEventListener("timeupdate", onTime);
      audio.removeEventListener("durationchange", onDuration);
      audio.removeEventListener("loadedmetadata", onDuration);
      audio.removeEventListener("play", onPlay);
      audio.removeEventListener("pause", onPause);
      audio.removeEventListener("ended", onEnded);
    };
  }, [completeAndNext]);

  // Load + play whenever the current track changes.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !current) return;
    audio.src = current.streamUrl ?? streamUrl(session, current.id);
    audio.play().catch(() => {
      /* autoplay/gesture rejection — the UI play button recovers */
    });
  }, [current, session]);

  // Endless Wave: ask the server for more when the queue is almost drained.
  useEffect(() => {
    if (!isWave || !current || extendingRef.current) return;
    if (queue.length - index > 3) return;
    extendingRef.current = true;
    getWaveNext(session, { seedId: current.id, count: 12 })
      .then((songs) => {
        if (songs.length === 0) return;
        setQueue((existing) => {
          const seen = new Set(existing.map((song) => song.id));
          const fresh = songs.filter((song) => !seen.has(song.id));
          return fresh.length ? [...existing, ...fresh] : existing;
        });
      })
      .finally(() => {
        extendingRef.current = false;
      });
  }, [current, index, isWave, queue.length, session]);

  // Media Session: lock-screen metadata + controls.
  useEffect(() => {
    if (!("mediaSession" in navigator) || !current) return;
    const art = cover(current.coverArt, 512);
    navigator.mediaSession.metadata = new MediaMetadata({
      title: current.title,
      artist: current.artist,
      album: current.album ?? "",
      artwork: art ? [{ src: art, sizes: "512x512", type: "image/jpeg" }] : [],
    });
    navigator.mediaSession.setActionHandler("play", () => audioRef.current?.play());
    navigator.mediaSession.setActionHandler("pause", () => audioRef.current?.pause());
    navigator.mediaSession.setActionHandler("nexttrack", () => next());
    navigator.mediaSession.setActionHandler("previoustrack", () => prev());
  }, [current, cover, next, prev]);

  // Seed liked-track ids once.
  useEffect(() => {
    let cancelled = false;
    getStarred(session)
      .then((starred) => {
        if (!cancelled) setStarredIds(new Set(starred.songs.map((s) => s.id)));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [session]);

  const playQueue = useCallback((songs: Song[], startIndex = 0) => {
    if (songs.length === 0) return;
    setIsWave(false);
    setQueue(songs);
    setIndex(Math.min(Math.max(startIndex, 0), songs.length - 1));
  }, []);

  const startWave = useCallback(async () => {
    const songs = await getWaveNext(session, { count: 12 });
    if (songs.length === 0) {
      throw new Error("Wave returned no tracks yet");
    }
    setIsWave(true);
    setQueue(songs);
    setIndex(0);
  }, [session]);

  const toggle = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    if (audio.paused) audio.play().catch(() => undefined);
    else audio.pause();
  }, []);

  const seek = useCallback((seconds: number) => {
    const audio = audioRef.current;
    if (audio) audio.currentTime = seconds;
  }, []);

  const isStarred = useCallback((id: string) => starredIds.has(id), [starredIds]);

  const toggleStar = useCallback(
    (id: string) => {
      setStarredIds((prevSet) => {
        const nextSet = new Set(prevSet);
        const wasStarred = nextSet.has(id);
        if (wasStarred) nextSet.delete(id);
        else nextSet.add(id);
        if (!wasStarred) {
          waveFeedback(session, id, "like").catch(() => undefined);
        }
        (wasStarred ? unstar(session, id) : star(session, id)).catch(() => {
          // revert on failure
          setStarredIds((s) => {
            const reverted = new Set(s);
            if (wasStarred) reverted.add(id);
            else reverted.delete(id);
            return reverted;
          });
        });
        return nextSet;
      });
    },
    [session],
  );

  const dislikeCurrent = useCallback(() => {
    if (!current) return;
    if (isWave) {
      waveFeedback(session, current.id, "dislike").catch(() => undefined);
    }
    advance("skip");
  }, [advance, current, isWave, session]);

  const value = useMemo<PlayerValue>(
    () => ({
      session,
      queue,
      index,
      current,
      isPlaying,
      currentTime,
      duration,
      isWave,
      playQueue,
      startWave,
      toggle,
      next,
      prev,
      seek,
      isStarred,
      toggleStar,
      dislikeCurrent,
      cover,
    }),
    [
      session,
      queue,
      index,
      current,
      isPlaying,
      currentTime,
      duration,
      isWave,
      playQueue,
      startWave,
      toggle,
      next,
      prev,
      seek,
      isStarred,
      toggleStar,
      dislikeCurrent,
      cover,
    ],
  );

  return (
    <PlayerContext.Provider value={value}>
      <audio ref={audioRef} preload="none" className="hidden" />
      {children}
    </PlayerContext.Provider>
  );
}

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const total = Math.floor(seconds);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
