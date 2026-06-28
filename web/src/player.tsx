import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  coverUrl,
  createListen,
  getLyrics,
  getListenState,
  getRemoteState,
  getWaveNext,
  joinListen as apiJoinListen,
  leaveListen as apiLeaveListen,
  listenCommand,
  remoteCommand,
  reportNowPlaying,
  streamUrl,
  waveFeedback,
} from "./api";
import type { WaveSession } from "./auth";
import { PlayerContext, type PlayerValue, type RepeatMode } from "./player-context";
import {
  audioDuration,
  clamp,
  estimateServerClockOffset,
  loadVolume,
  remotePlaybackSeconds,
  saveVolume,
  toRemoteTrack,
} from "./player-utils";
import { useStarredTracks } from "./player-stars";
import { useDownloads } from "./downloads";
import { useOnlineStatus } from "./online";
import { getStreamQuality } from "./quality";
import type { ListenState, RemoteState, Song } from "./types";

export { usePlayer } from "./player-context";
export { formatTime } from "./player-utils";

export function PlayerProvider({
  session,
  children,
}: {
  session: WaveSession;
  children: ReactNode;
}) {
  // Active playback element plus hidden ones warming upcoming tracks. The
  // preload requests also make the server resolve/stage virtual tracks ahead
  // of time, so starting/advancing swaps elements instead of waiting on the
  // pipeline. Capped at 2 — the server allows 3 concurrent virtual streams,
  // and the active track needs a slot.
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const preloadsRef = useRef<Map<string, HTMLAudioElement>>(new Map());
  const detachRef = useRef<(() => void) | null>(null);
  const retryLosslessRef = useRef<(audio: HTMLAudioElement) => boolean>(() => false);
  const losslessFallbackIdsRef = useRef<Set<string>>(new Set());
  const timingFrameRef = useRef<number | null>(null);
  // The current track's known metadata duration. Kept in a ref so the audio
  // element's timing listeners (attached once, reused across tracks) always see
  // a stable duration instead of the streamed source's growing audio.duration.
  const fallbackDurationRef = useRef<number | undefined>(undefined);
  const wavePrefetchRef = useRef<{ promise: Promise<Song[]>; at: number } | null>(null);
  const sessionRef = useRef(session);
  sessionRef.current = session;

  const [queue, setQueue] = useState<Song[]>([]);
  const [index, setIndex] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isWave, setIsWave] = useState(false);
  const [repeat, setRepeat] = useState<RepeatMode>("off");
  const [shuffle, setShuffle] = useState(false);
  const [volume, setVolumeState] = useState(loadVolume);
  const [muted, setMuted] = useState(false);
  // Remote control (Spotify-Connect-style: app drives the Discord bot).
  const [remoteOn, setRemoteOn] = useState(false);
  const [remoteState, setRemoteState] = useState<RemoteState | null>(null);
  const [, bumpRemoteClock] = useState(0); // re-render to interpolate progress
  const remoteOnRef = useRef(remoteOn);
  remoteOnRef.current = remoteOn;
  const remoteStateRef = useRef(remoteState);
  remoteStateRef.current = remoteState;
  const extendingRef = useRef(false);
  // Listen Together (shared browser session).
  const [listenCode, setListenCode] = useState<string | null>(null);
  const [listenState, setListenState] = useState<ListenState | null>(null);
  const [listenClockOffsetMs, setListenClockOffsetMs] = useState(0);
  const [, setListenNeedsUnlock] = useState(false);
  const listenCodeRef = useRef<string | null>(null);
  const listenStateRef = useRef<ListenState | null>(null);
  listenCodeRef.current = listenCode;
  listenStateRef.current = listenState;

  const downloads = useDownloads();
  const online = useOnlineStatus();
  const { isStarred, toggleStar } = useStarredTracks(session);
  const getPlayableUrl = downloads.getPlayableUrl; // stable identity
  // Latest values for callbacks/listeners that must stay stable across renders.
  const downloadsRef = useRef(downloads);
  downloadsRef.current = downloads;
  const repeatRef = useRef(repeat);
  const queueRef = useRef(queue);
  queueRef.current = queue;
  const indexRef = useRef(index);
  indexRef.current = index;
  const shuffleRef = useRef(shuffle);
  shuffleRef.current = shuffle;
  // Pre-shuffle order, so toggling shuffle off restores the original sequence.
  const preShuffleRef = useRef<Song[] | null>(null);

  const current = queue[index] ?? null;
  const currentRef = useRef<Song | null>(null);
  currentRef.current = current;

  retryLosslessRef.current = (audio) => {
    const song = currentRef.current;
    if (!song || losslessFallbackIdsRef.current.has(song.id)) return false;
    const wasOfflineBlob = audio.src.startsWith("blob:");
    if (getStreamQuality() !== "lossless" && !wasOfflineBlob) return false;
    losslessFallbackIdsRef.current.add(song.id);
    console.info("lossless/original stream failed; retrying mp3 fallback", song.id);
    audio.src = streamUrl(sessionRef.current, song.id, "high");
    audio.load();
    if (!listenCodeRef.current || listenStateRef.current?.isPlaying) {
      audio.play().catch(() => {
        if (listenCodeRef.current) setListenNeedsUnlock(true);
      });
    }
    return true;
  };

  const cover = useCallback(
    (coverArt: string | undefined, size = 200) => coverUrl(session, coverArt, size),
    [session],
  );

  const dispatchRemote = useCallback(
    (action: string, payload?: unknown) => {
      remoteCommand(session, action, payload).catch(() => undefined);
    },
    [session],
  );

  const dispatchListen = useCallback(
    (action: string, payload?: unknown) => {
      const code = listenCodeRef.current;
      if (!code) return;
      listenCommand(session, code, action, payload).catch(() => undefined);
    },
    [session],
  );

  const estimateListenClockOffset = useCallback(async () => {
    setListenClockOffsetMs(await estimateServerClockOffset(session));
  }, [session]);

  /** Warm the browser cache with a song's covers: the full-screen size and
   *  the list/backdrop size. */
  const warmCovers = useCallback(
    (song: Song | undefined) => {
      if (!song?.coverArt) return;
      for (const size of [600, 80]) {
        const url = cover(song.coverArt, size);
        if (url) new Image().src = url;
      }
    },
    [cover],
  );

  /** Start buffering a song's audio in a hidden element (keeps at most
   *  `keepIds` + the new one alive; evicted elements stop downloading). */
  const warmAudio = useCallback(
    (song: Song | undefined, keepIds: string[] = []) => {
      if (!song) return;
      // Downloaded tracks play from the local blob (instant, offline), so don't
      // burn a network preload slot on them — the load effect uses the blob.
      if (downloadsRef.current.isDownloaded(song.id)) return;
      const preloads = preloadsRef.current;
      if (preloads.has(song.id)) return;
      for (const [id, audio] of preloads) {
        if (preloads.size < 2) break;
        if (id === song.id || keepIds.includes(id)) continue;
        audio.removeAttribute("src");
        audio.load();
        preloads.delete(id);
      }
      if (preloads.size >= 2) return;
      const audio = new Audio();
      audio.preload = "auto";
      audio.volume = volume;
      audio.muted = muted;
      audio.src = song.streamUrl ?? streamUrl(session, song.id);
      audio.load();
      preloads.set(song.id, audio);
    },
    [muted, session, volume],
  );

  const advance = useCallback(
    (action: "play" | "skip") => {
      const track = queue[index];
      if (isWave && track) {
        waveFeedback(session, track.id, action).catch(() => undefined);
      }
      setIndex((i) => {
        if (i + 1 < queue.length) return i + 1;
        return repeatRef.current === "all" ? 0 : i; // wrap to start on repeat-all
      });
    },
    [index, isWave, queue, session],
  );

  const next = useCallback(() => {
    if (remoteOnRef.current) {
      dispatchRemote("next");
      return;
    }
    if (listenCodeRef.current) {
      dispatchListen("next");
      return;
    }
    advance("skip");
  }, [advance, dispatchListen, dispatchRemote]);

  const completeAndNext = useCallback(() => {
    if (listenCodeRef.current) return;
    advance("play");
  }, [advance]);

  const prev = useCallback(() => {
    if (remoteOnRef.current) {
      dispatchRemote("prev");
      return;
    }
    if (listenCodeRef.current) {
      const audio = audioRef.current;
      if (audio && audio.currentTime > 3) {
        dispatchListen("seek", { positionMs: 0 });
      } else {
        dispatchListen("prev");
      }
      return;
    }
    const audio = audioRef.current;
    if (audio && audio.currentTime > 3) {
      audio.currentTime = 0;
      return;
    }
    setIndex((i) => (i > 0 ? i - 1 : i));
  }, [dispatchListen, dispatchRemote]);

  // `ended` must always advance from the latest queue state, while the
  // attached listeners stay stable across element swaps — bridge via a ref.
  const advanceRef = useRef<() => void>(() => undefined);
  useEffect(() => {
    advanceRef.current = completeAndNext;
  }, [completeAndNext]);

  /** Make `audio` the active element: move listeners and state over to it. */
  const attach = useCallback((audio: HTMLAudioElement, fallbackDuration?: number) => {
    detachRef.current?.();
    fallbackDurationRef.current = fallbackDuration;
    const stopSmoothTiming = () => {
      if (timingFrameRef.current !== null) {
        window.cancelAnimationFrame(timingFrameRef.current);
        timingFrameRef.current = null;
      }
    };
    const syncTiming = () => {
      setCurrentTime(audio.currentTime);
      setDuration(audioDuration(audio, fallbackDurationRef.current));
    };
    const onError = () => {
      syncTiming();
      retryLosslessRef.current(audio);
    };
    const tickTiming = () => {
      syncTiming();
      if (!audio.paused && !audio.ended) {
        timingFrameRef.current = window.requestAnimationFrame(tickTiming);
      }
    };
    const startSmoothTiming = () => {
      stopSmoothTiming();
      if (!audio.paused && !audio.ended) {
        timingFrameRef.current = window.requestAnimationFrame(tickTiming);
      }
    };
    const syncPlayback = () => {
      const playing = !audio.paused && !audio.ended;
      setIsPlaying(playing);
      if (playing) startSmoothTiming();
      else stopSmoothTiming();
    };
    const onEnded = () => {
      syncPlayback();
      advanceRef.current();
    };
    audio.addEventListener("timeupdate", syncTiming);
    audio.addEventListener("durationchange", syncTiming);
    audio.addEventListener("loadedmetadata", syncTiming);
    audio.addEventListener("loadeddata", syncTiming);
    audio.addEventListener("canplay", syncTiming);
    audio.addEventListener("emptied", syncTiming);
    audio.addEventListener("error", onError);
    audio.addEventListener("play", syncPlayback);
    audio.addEventListener("playing", syncPlayback);
    audio.addEventListener("pause", syncPlayback);
    audio.addEventListener("ended", onEnded);
    audioRef.current = audio;
    audio.loop = repeatRef.current === "one"; // repeat-one loops without 'ended'
    detachRef.current = () => {
      stopSmoothTiming();
      audio.removeEventListener("timeupdate", syncTiming);
      audio.removeEventListener("durationchange", syncTiming);
      audio.removeEventListener("loadedmetadata", syncTiming);
      audio.removeEventListener("loadeddata", syncTiming);
      audio.removeEventListener("canplay", syncTiming);
      audio.removeEventListener("emptied", syncTiming);
      audio.removeEventListener("error", onError);
      audio.removeEventListener("play", syncPlayback);
      audio.removeEventListener("playing", syncPlayback);
      audio.removeEventListener("pause", syncPlayback);
      audio.removeEventListener("ended", onEnded);
    };
    syncTiming();
    syncPlayback();
  }, []);

  useEffect(() => {
    const audio = new Audio();
    audio.preload = "auto";
    audio.volume = volume;
    audio.muted = muted;
    attach(audio);
    return () => {
      detachRef.current?.();
      audio.pause();
      audio.removeAttribute("src");
      for (const preloaded of preloadsRef.current.values()) {
        preloaded.removeAttribute("src");
      }
      preloadsRef.current.clear();
    };
  }, [attach]);

  useEffect(() => {
    saveVolume(volume);
    for (const audio of [audioRef.current, ...preloadsRef.current.values()]) {
      if (!audio) continue;
      audio.volume = volume;
      audio.muted = muted;
    }
  }, [muted, volume]);

  // Load + play whenever the current track changes; if the track was already
  // preloaded, swap elements and start instantly.
  useEffect(() => {
    if (!current || remoteOnRef.current) return; // remote: the bot plays, not us
    fallbackDurationRef.current = current.duration; // stable denominator for the progress bar
    const preloaded = preloadsRef.current.get(current.id);
    if (preloaded) {
      preloadsRef.current.delete(current.id);
      const old = audioRef.current;
      attach(preloaded, current.duration);
      if (old) {
        old.pause();
        old.removeAttribute("src");
      }
      if (!listenCodeRef.current || listenStateRef.current?.isPlaying) {
        preloaded.play().catch(() => {
          if (listenCodeRef.current) setListenNeedsUnlock(true);
          /* autoplay/gesture rejection — the UI play button recovers */
        });
      } else {
        preloaded.pause();
      }
      return;
    }
    const audio = audioRef.current;
    if (!audio) return;
    setCurrentTime(0);
    setDuration(current.duration ?? 0);
    let cancelled = false;
    // Prefer an offline copy (instant + works with no network); fall back to
    // the network stream. getPlayableUrl resolves null for non-downloaded ids.
    getPlayableUrl(current.id).then((localUrl) => {
      if (cancelled || audioRef.current !== audio) return;
      audio.src = localUrl ?? current.streamUrl ?? streamUrl(session, current.id);
      audio.load();
      if (!listenCodeRef.current || listenStateRef.current?.isPlaying) {
        audio.play().catch(() => {
          if (listenCodeRef.current) setListenNeedsUnlock(true);
          /* autoplay/gesture rejection — the UI play button recovers */
        });
      }
    });
    return () => {
      cancelled = true;
    };
  }, [current, session, attach, getPlayableUrl]);

  // Warm the next track (audio), the large covers, and the lyrics of the
  // current and next tracks, so advancing, opening the full-screen player,
  // and the lyrics button are all instant. getLyrics memoizes per song id.
  useEffect(() => {
    if (remoteOnRef.current) return; // remote: no local preloading
    const nextSong = queue[index + 1];
    warmAudio(nextSong);
    warmCovers(current ?? undefined);
    warmCovers(nextSong);
    if (current) getLyrics(session, current.id).catch(() => undefined);
    if (nextSong) getLyrics(session, nextSong.id).catch(() => undefined);
  }, [queue, index, current, session, warmAudio, warmCovers]);

  // Fetch the first Wave batch and warm its opening tracks + covers, so the
  // big Wave button starts music with no wait. Refreshed when the app comes
  // back to the foreground after sitting idle, so a stale batch never plays.
  const prefetchWave = useCallback(() => {
    const promise = getWaveNext(session, { count: 12 }).catch(() => [] as Song[]);
    wavePrefetchRef.current = { promise, at: Date.now() };
    promise.then((songs) => {
      // Skip the warm-up once something is playing — the queue effect owns
      // the preload slots then.
      if (audioRef.current?.src) return;
      warmAudio(songs[0]);
      warmAudio(songs[1], songs[0] ? [songs[0].id] : []);
      warmCovers(songs[0]);
      warmCovers(songs[1]);
    });
  }, [session, warmAudio, warmCovers]);

  useEffect(() => {
    prefetchWave();
    const WAVE_PREFETCH_TTL_MS = 10 * 60 * 1000;
    const onVisible = () => {
      if (document.visibilityState !== "visible") return;
      if (audioRef.current?.src) return; // playing — nothing to refresh
      const prefetched = wavePrefetchRef.current;
      if (prefetched && Date.now() - prefetched.at < WAVE_PREFETCH_TTL_MS) return;
      prefetchWave();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [prefetchWave]);

  // Endless Wave: ask the server for more when the queue is almost drained.
  useEffect(() => {
    if (!isWave || !current || extendingRef.current || remoteOnRef.current || listenCodeRef.current) return;
    if (queue.length - index > 3) return;
    extendingRef.current = true;
    getWaveNext(session, { seedId: current.id, count: 12 })
      .then(async (songs) => {
        if (songs.length === 0) {
          songs = await getWaveNext(session, { count: 12 }).catch(() => [] as Song[]);
        }
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
    // 600 matches the full-screen player, so the lock screen reuses its cache.
    const art = cover(current.coverArt, 600);
    navigator.mediaSession.metadata = new MediaMetadata({
      title: current.title,
      artist: current.artist,
      album: current.album ?? "",
      artwork: art ? [{ src: art, sizes: "600x600", type: "image/jpeg" }] : [],
    });
    navigator.mediaSession.setActionHandler("play", () => audioRef.current?.play());
    navigator.mediaSession.setActionHandler("pause", () => audioRef.current?.pause());
    navigator.mediaSession.setActionHandler("nexttrack", () => next());
    navigator.mediaSession.setActionHandler("previoustrack", () => prev());
  }, [current, cover, next, prev]);

  // Friend Activity: report the current track whenever it changes. While remote,
  // the bot reports state instead (the proxy mirrors it into now-playing).
  useEffect(() => {
    if (!current || remoteOnRef.current) return;
    reportNowPlaying(session, current).catch(() => undefined);
  }, [current, session]);

  // Remote: long-poll the bot's playback state while connected, so the playbar
  // reflects skips/track-changes near-instantly instead of on a slow interval.
  useEffect(() => {
    if (!remoteOn) return;
    let active = true;
    const controller = new AbortController();
    let since = 0;
    (async () => {
      while (active) {
        try {
          const state = await getRemoteState(session, {
            wait: 25,
            since,
            signal: controller.signal,
          });
          if (!active) break;
          since = state.rev;
          setRemoteState(state);
        } catch {
          if (!active) break;
          await new Promise((resolve) => setTimeout(resolve, 2000)); // backoff
        }
      }
    })();
    return () => {
      active = false;
      controller.abort();
    };
  }, [remoteOn, session]);

  // Listen Together: long-poll shared room revisions and mirror the shared
  // timeline into local playback state.
  useEffect(() => {
    if (!listenCode) return;
    let active = true;
    const controller = new AbortController();
    let since = 0;
    estimateListenClockOffset().catch(() => undefined);
    (async () => {
      while (active) {
        try {
          const state = await getListenState(session, {
            code: listenCode,
            since,
            wait: 25,
            signal: controller.signal,
          });
          if (!active) break;
          if (!state) {
            setListenCode(null);
            setListenState(null);
            break;
          }
          since = state.rev;
          setListenState(state);
        } catch {
          if (!active) break;
          await new Promise((resolve) => setTimeout(resolve, 2000));
        }
      }
    })();
    return () => {
      active = false;
      controller.abort();
    };
  }, [estimateListenClockOffset, listenCode, session]);

  useEffect(() => {
    const state = listenState;
    if (!listenCode || !state) return;
    setIsWave(false);
    if (!state.track) {
      audioRef.current?.pause();
      setQueue([]);
      setIndex(0);
      return;
    }
    const nextQueue = [state.track, ...state.queue];
    setQueue((existing) => {
      if (
        existing.length === nextQueue.length &&
        existing.every((song, i) => song.id === nextQueue[i]?.id)
      ) {
        return existing;
      }
      return nextQueue;
    });
    setIndex((i) => (i === 0 ? i : 0));
  }, [listenCode, listenState]);

  // Listen Together: keep the audio element close to the shared server-clock
  // timeline. Big drift seeks; small drift gently nudges playbackRate.
  useEffect(() => {
    if (!listenCode || !listenState?.track) return;
    const sync = () => {
      const audio = audioRef.current;
      const state = listenStateRef.current;
      if (!audio || !state?.track) return;
      const targetMs = state.isPlaying
        ? state.anchorPosMs + (Date.now() + listenClockOffsetMs - state.anchorTsMs)
        : state.anchorPosMs;
      const targetSec = Math.max(0, targetMs / 1000);
      const drift = targetSec - audio.currentTime;
      if (Math.abs(drift) > 0.75) {
        audio.currentTime = targetSec;
        audio.playbackRate = 1;
      } else if (state.isPlaying && Math.abs(drift) > 0.08) {
        audio.playbackRate = clamp(1 + drift * 0.04, 0.97, 1.03);
      } else {
        audio.playbackRate = 1;
      }
      if (state.isPlaying) {
        audio
          .play()
          .then(() => setListenNeedsUnlock(false))
          .catch(() => setListenNeedsUnlock(true));
      } else {
        setListenNeedsUnlock(false);
        audio.pause();
      }
    };
    sync();
    const id = window.setInterval(sync, 500);
    return () => {
      window.clearInterval(id);
      if (audioRef.current) audioRef.current.playbackRate = 1;
    };
  }, [listenClockOffsetMs, listenCode, listenState?.rev, listenState?.track?.id]);

  // Remote: tick a few times a second so the progress bar interpolates between
  // the (slower) state polls.
  useEffect(() => {
    if (!remoteOn || !remoteState?.isPlaying) return;
    const id = window.setInterval(() => bumpRemoteClock((n) => n + 1), 500);
    return () => window.clearInterval(id);
  }, [remoteOn, remoteState?.isPlaying]);

  // Remote: silence local audio while a remote device is in control.
  useEffect(() => {
    if (remoteOn) audioRef.current?.pause();
  }, [remoteOn]);

  // Keep repeat-one's gapless loop flag in sync with the live element.
  useEffect(() => {
    repeatRef.current = repeat;
    if (audioRef.current) audioRef.current.loop = repeat === "one";
  }, [repeat]);

  const cycleRepeat = useCallback(() => {
    setRepeat((mode) => (mode === "off" ? "all" : mode === "all" ? "one" : "off"));
  }, []);

  const toggleShuffle = useCallback(() => {
    const turningOn = !shuffleRef.current;
    const q = queueRef.current;
    const idx = indexRef.current;
    const playing = q[idx];
    setShuffle(turningOn);
    if (turningOn) {
      preShuffleRef.current = q;
      const head = q.slice(0, idx + 1);
      const tail = q.slice(idx + 1);
      for (let i = tail.length - 1; i > 0; i -= 1) {
        const j = Math.floor(Math.random() * (i + 1));
        [tail[i], tail[j]] = [tail[j], tail[i]];
      }
      setQueue([...head, ...tail]);
    } else {
      const original = preShuffleRef.current ?? q;
      preShuffleRef.current = null;
      const restoredIndex = playing
        ? original.findIndex((song) => song.id === playing.id)
        : idx;
      setQueue(original);
      setIndex(restoredIndex >= 0 ? restoredIndex : idx);
    }
  }, []);

  const moreLikeCurrent = useCallback(async () => {
    const song = currentRef.current;
    if (!song || remoteOnRef.current || listenCodeRef.current) return;
    const songs = await getWaveNext(sessionRef.current, { seedId: song.id, count: 12 });
    if (songs.length === 0) return;
    setIsWave(true);
    setShuffle(false);
    preShuffleRef.current = null;
    setQueue((queue) => {
      const currentSong = queue[indexRef.current] ?? song;
      return [currentSong, ...songs.filter((next) => next.id !== currentSong.id)];
    });
    setIndex(0);
  }, []);

  const setVolume = useCallback((value: number) => {
    const next = clamp(value, 0, 1);
    setVolumeState(next);
    if (next > 0) setMuted(false);
  }, []);

  const toggleMute = useCallback(() => {
    setMuted((value) => !value);
  }, []);

  const playQueue = useCallback(
    (songs: Song[], startIndex = 0) => {
      if (songs.length === 0) return;
      if (remoteOnRef.current) {
        dispatchRemote("play", { tracks: songs.map(toRemoteTrack), startIndex });
        return;
      }
      if (listenCodeRef.current) {
        const safeIndex = Math.min(Math.max(startIndex, 0), songs.length - 1);
        const selected = songs[safeIndex];
        setIsWave(false);
        setShuffle(false);
        preShuffleRef.current = null;
        setQueue(songs);
        setIndex(safeIndex);
        setListenState((state) =>
          state
            ? {
                ...state,
                track: selected,
                queue: songs.slice(safeIndex + 1),
                anchorPosMs: 0,
                anchorTsMs: Date.now() + listenClockOffsetMs,
                isPlaying: true,
                rev: state.rev + 1,
              }
            : state,
        );
        const audio = audioRef.current;
        if (audio) {
          audio.src = selected.streamUrl ?? streamUrl(session, selected.id);
          audio.load();
          audio
            .play()
            .then(() => setListenNeedsUnlock(false))
            .catch(() => setListenNeedsUnlock(true));
        }
        dispatchListen("play", { tracks: songs.map(toRemoteTrack), startIndex });
        return;
      }
      setIsWave(false);
      setShuffle(false);
      preShuffleRef.current = null;
      setQueue(songs);
      setIndex(Math.min(Math.max(startIndex, 0), songs.length - 1));
    },
    [dispatchListen, dispatchRemote, listenClockOffsetMs, session],
  );

  const connectRemote = useCallback(() => {
    const code = listenCodeRef.current;
    if (code) {
      apiLeaveListen(session, code).catch(() => undefined);
      setListenCode(null);
      setListenState(null);
    }
    audioRef.current?.pause();
    setRemoteOn(true);
    dispatchRemote("connect");
  }, [dispatchRemote, session]);

  const disconnectRemote = useCallback(() => {
    dispatchRemote("disconnect");
    setRemoteOn(false);
    setRemoteState(null);
  }, [dispatchRemote]);

  const startListen = useCallback(async () => {
    if (remoteOnRef.current) {
      dispatchRemote("disconnect");
      setRemoteOn(false);
      setRemoteState(null);
    }
    const code = await createListen(session);
    const state = await apiJoinListen(session, code);
    setListenCode(code);
    setListenState(state);
    await estimateListenClockOffset().catch(() => undefined);
    return code;
  }, [dispatchRemote, estimateListenClockOffset, session]);

  const joinListen = useCallback(
    async (code: string) => {
      const normalized = code.trim().toUpperCase();
      if (!normalized) return;
      if (remoteOnRef.current) {
        dispatchRemote("disconnect");
        setRemoteOn(false);
        setRemoteState(null);
      }
      const state = await apiJoinListen(session, normalized);
      setListenCode(normalized);
      setListenState(state);
      await estimateListenClockOffset().catch(() => undefined);
    },
    [dispatchRemote, estimateListenClockOffset, session],
  );

  const leaveListen = useCallback(() => {
    const code = listenCodeRef.current;
    if (code) apiLeaveListen(session, code).catch(() => undefined);
    setListenCode(null);
    setListenState(null);
    if (audioRef.current) audioRef.current.playbackRate = 1;
  }, [session]);

  const sendListenReaction = useCallback(
    (emoji: string) => dispatchListen("reaction", { emoji }),
    [dispatchListen],
  );

  const sendListenChat = useCallback(
    (text: string) => dispatchListen("chat", { text }),
    [dispatchListen],
  );

  const startWave = useCallback(async () => {
    // Remote: let the bot run its own endless Wave (refills server-side).
    if (remoteOnRef.current) {
      dispatchRemote("wave");
      return;
    }
    if (listenCodeRef.current) {
      await listenCommand(session, listenCodeRef.current, "wave");
      return;
    }
    if (!online) {
      const downloaded = downloads.downloadedSongs;
      if (downloaded.length === 0) {
        throw new Error("offline-no-downloads");
      }
      const songs = [...downloaded].sort(() => Math.random() - 0.5);
      setIsWave(false);
      setShuffle(false);
      preShuffleRef.current = null;
      setQueue(songs);
      setIndex(0);
      return;
    }
    // Use the batch prefetched at startup when available; fetch fresh after
    // it's consumed so repeated taps still get current recommendations.
    const prefetched = wavePrefetchRef.current;
    wavePrefetchRef.current = null;
    let songs = prefetched ? await prefetched.promise : [];
    if (songs.length === 0) {
      songs = await getWaveNext(session, { count: 12 });
    }
    if (songs.length === 0) {
      throw new Error("Wave returned no tracks yet");
    }
    setIsWave(true);
    setShuffle(false);
    preShuffleRef.current = null;
    setQueue(songs);
    setIndex(0);
  }, [session, dispatchRemote, downloads.downloadedSongs, online]);

  const toggle = useCallback(() => {
    if (remoteOnRef.current) {
      const playing = remoteStateRef.current?.isPlaying ?? false;
      dispatchRemote(playing ? "pause" : "resume");
      setRemoteState((s) => (s ? { ...s, isPlaying: !playing, fetchedAt: Date.now() } : s));
      return;
    }
    if (listenCodeRef.current) {
      const audio = audioRef.current;
      if (listenStateRef.current?.isPlaying && audio?.paused) {
        audio
          .play()
          .then(() => setListenNeedsUnlock(false))
          .catch(() => setListenNeedsUnlock(true));
        return;
      }
      const playing = listenStateRef.current?.isPlaying ?? isPlaying;
      dispatchListen(playing ? "pause" : "resume");
      return;
    }
    const audio = audioRef.current;
    if (!audio) return;
    if (audio.paused) audio.play().catch(() => undefined);
    else audio.pause();
  }, [dispatchListen, dispatchRemote, isPlaying]);

  const seek = useCallback(
    (seconds: number) => {
      if (remoteOnRef.current) {
        const positionMs = Math.round(Math.max(seconds, 0) * 1000);
        dispatchRemote("seek", { positionMs });
        setRemoteState((s) => (s ? { ...s, positionMs, fetchedAt: Date.now() } : s));
        return;
      }
      if (listenCodeRef.current) {
        dispatchListen("seek", { positionMs: Math.round(Math.max(seconds, 0) * 1000) });
        return;
      }
      const audio = audioRef.current;
      if (audio) audio.currentTime = seconds;
    },
    [dispatchListen, dispatchRemote],
  );

  const dislikeCurrent = useCallback(() => {
    if (remoteOnRef.current) {
      dispatchRemote("next");
      return;
    }
    if (listenCodeRef.current) {
      dispatchListen("reaction", { emoji: "👎" });
      dispatchListen("next");
      return;
    }
    if (!current) return;
    if (isWave) {
      waveFeedback(session, current.id, "dislike").catch(() => undefined);
    }
    advance("skip");
  }, [advance, current, dispatchListen, dispatchRemote, isWave, session]);

  // Exposed playback view: the bot's reported state while remote, else local.
  // Remote position is interpolated from the last poll so the bar moves live.
  const remoteSecs = remotePlaybackSeconds(remoteOn, remoteState);
  const exposedCurrent = remoteOn ? remoteState?.song ?? null : current;
  const exposedIsPlaying = remoteOn ? remoteState?.isPlaying ?? false : isPlaying;
  const exposedTime = remoteOn ? remoteSecs : currentTime;
  const exposedDuration = remoteOn ? (remoteState?.durationMs ?? 0) / 1000 : duration;
  const exposedQueue = remoteOn
    ? remoteState?.song
      ? [remoteState.song, ...(remoteState.queue ?? [])]
      : remoteState?.queue ?? []
    : queue;
  const exposedIndex = remoteOn ? 0 : index;
  const remoteConnected = remoteState?.connected ?? false;
  const remoteBusy = remoteOn && (remoteState?.busy ?? false);
  const listenMembers = listenState?.members ?? [];
  const listenEvents = listenState?.events ?? [];

  const value = useMemo<PlayerValue>(
    () => ({
      session,
      queue: exposedQueue,
      index: exposedIndex,
      current: exposedCurrent,
      isPlaying: exposedIsPlaying,
      currentTime: exposedTime,
      duration: exposedDuration,
      isWave,
      repeat,
      shuffle,
      volume,
      muted,
      remoteOn,
      remoteConnected,
      remoteBusy,
      connectRemote,
      disconnectRemote,
      listenCode,
      listenMembers,
      listenEvents,
      startListen,
      joinListen,
      leaveListen,
      sendListenReaction,
      sendListenChat,
      playQueue,
      startWave,
      toggle,
      next,
      prev,
      seek,
      cycleRepeat,
      toggleShuffle,
      moreLikeCurrent,
      setVolume,
      toggleMute,
      isStarred,
      toggleStar,
      dislikeCurrent,
      cover,
    }),
    [
      session,
      exposedQueue,
      exposedIndex,
      exposedCurrent,
      exposedIsPlaying,
      exposedTime,
      exposedDuration,
      isWave,
      repeat,
      shuffle,
      volume,
      muted,
      remoteOn,
      remoteConnected,
      remoteBusy,
      connectRemote,
      disconnectRemote,
      listenCode,
      listenMembers,
      listenEvents,
      startListen,
      joinListen,
      leaveListen,
      sendListenReaction,
      sendListenChat,
      playQueue,
      startWave,
      toggle,
      next,
      prev,
      seek,
      cycleRepeat,
      toggleShuffle,
      moreLikeCurrent,
      setVolume,
      toggleMute,
      isStarred,
      toggleStar,
      dislikeCurrent,
      cover,
    ],
  );

  return <PlayerContext.Provider value={value}>{children}</PlayerContext.Provider>;
}
