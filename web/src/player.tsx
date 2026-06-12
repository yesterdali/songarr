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
  // Active playback element plus a hidden one warming the next track. The
  // preload request also makes the server resolve/stage virtual tracks ahead
  // of time, so advancing swaps elements instead of waiting on the pipeline.
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const preloadRef = useRef<{ id: string; audio: HTMLAudioElement } | null>(null);
  const detachRef = useRef<(() => void) | null>(null);
  const wavePrefetchRef = useRef<Promise<Song[]> | null>(null);

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

  // `ended` must always advance from the latest queue state, while the
  // attached listeners stay stable across element swaps — bridge via a ref.
  const advanceRef = useRef<() => void>(() => undefined);
  useEffect(() => {
    advanceRef.current = completeAndNext;
  }, [completeAndNext]);

  /** Make `audio` the active element: move listeners and state over to it. */
  const attach = useCallback((audio: HTMLAudioElement) => {
    detachRef.current?.();
    const onTime = () => setCurrentTime(audio.currentTime);
    const onDuration = () => setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
    const onPlay = () => setIsPlaying(true);
    const onPause = () => setIsPlaying(false);
    const onEnded = () => advanceRef.current();
    audio.addEventListener("timeupdate", onTime);
    audio.addEventListener("durationchange", onDuration);
    audio.addEventListener("loadedmetadata", onDuration);
    audio.addEventListener("play", onPlay);
    audio.addEventListener("pause", onPause);
    audio.addEventListener("ended", onEnded);
    audioRef.current = audio;
    detachRef.current = () => {
      audio.removeEventListener("timeupdate", onTime);
      audio.removeEventListener("durationchange", onDuration);
      audio.removeEventListener("loadedmetadata", onDuration);
      audio.removeEventListener("play", onPlay);
      audio.removeEventListener("pause", onPause);
      audio.removeEventListener("ended", onEnded);
    };
    setCurrentTime(audio.currentTime);
    setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
  }, []);

  useEffect(() => {
    const audio = new Audio();
    audio.preload = "auto";
    attach(audio);
    return () => {
      detachRef.current?.();
      audio.pause();
      audio.removeAttribute("src");
      if (preloadRef.current) {
        preloadRef.current.audio.removeAttribute("src");
        preloadRef.current = null;
      }
    };
  }, [attach]);

  // Load + play whenever the current track changes; if the track was already
  // preloaded, swap elements and start instantly.
  useEffect(() => {
    if (!current) return;
    const preloaded = preloadRef.current;
    if (preloaded && preloaded.id === current.id) {
      preloadRef.current = null;
      const old = audioRef.current;
      attach(preloaded.audio);
      if (old) {
        old.pause();
        old.removeAttribute("src");
      }
      preloaded.audio.play().catch(() => {
        /* autoplay/gesture rejection — the UI play button recovers */
      });
      return;
    }
    const audio = audioRef.current;
    if (!audio) return;
    audio.src = current.streamUrl ?? streamUrl(session, current.id);
    audio.play().catch(() => {
      /* autoplay/gesture rejection — the UI play button recovers */
    });
  }, [current, session, attach]);

  // Warm the next track in the queue so advancing is instant.
  useEffect(() => {
    const nextSong = queue[index + 1];
    if (!nextSong || preloadRef.current?.id === nextSong.id) return;
    if (preloadRef.current) {
      preloadRef.current.audio.removeAttribute("src");
      preloadRef.current.audio.load();
    }
    const audio = new Audio();
    audio.preload = "auto";
    audio.src = nextSong.streamUrl ?? streamUrl(session, nextSong.id);
    audio.load();
    preloadRef.current = { id: nextSong.id, audio };
  }, [queue, index, session]);

  // Fetch the first Wave batch at startup and warm its opening track, so the
  // big Wave button starts music with no wait.
  useEffect(() => {
    const prefetch = getWaveNext(session, { count: 12 }).catch(() => [] as Song[]);
    wavePrefetchRef.current = prefetch;
    prefetch.then((songs) => {
      const first = songs[0];
      // Don't clobber a queue preload, and skip once something is playing.
      if (!first || preloadRef.current || audioRef.current?.src) return;
      const audio = new Audio();
      audio.preload = "auto";
      audio.src = first.streamUrl ?? streamUrl(session, first.id);
      audio.load();
      preloadRef.current = { id: first.id, audio };
    });
  }, [session]);

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
    // Use the batch prefetched at startup when available; fetch fresh after
    // it's consumed so repeated taps still get current recommendations.
    const prefetched = wavePrefetchRef.current;
    wavePrefetchRef.current = null;
    let songs = prefetched ? await prefetched : [];
    if (songs.length === 0) {
      songs = await getWaveNext(session, { count: 12 });
    }
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

  return <PlayerContext.Provider value={value}>{children}</PlayerContext.Provider>;
}

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const total = Math.floor(seconds);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
