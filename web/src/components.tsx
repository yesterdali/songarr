import { useEffect, useState, type CSSProperties } from "react";
import {
  BanIcon,
  ChevronLeftIcon,
  HeartIcon,
  MusicNoteIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
  QueueIcon,
} from "./icons";
import { useNav } from "./nav";
import { formatTime, usePlayer } from "./player";
import type { Song } from "./types";

export function Cover({
  coverArt,
  size = 200,
  className = "",
  rounded = "rounded-xl",
  placeholderSize,
}: {
  coverArt?: string;
  size?: number;
  className?: string;
  rounded?: string;
  /** Show this (usually browser-cached) smaller size blurred-up while the
   *  full-size image is still downloading. */
  placeholderSize?: number;
}) {
  const { cover } = usePlayer();
  const [failed, setFailed] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const src = cover(coverArt, size);
  const placeholderSrc = placeholderSize ? cover(coverArt, placeholderSize) : undefined;
  useEffect(() => {
    setFailed(false);
    setLoaded(false);
  }, [coverArt]);
  if (!src || failed) {
    return (
      <div
        className={`grid place-items-center bg-gradient-to-br from-wave-pink/70 to-wave-violet/80 ${rounded} ${className}`}
      >
        <MusicNoteIcon className="h-2/5 w-2/5 text-white/60" />
      </div>
    );
  }
  if (!placeholderSrc) {
    return (
      <img
        src={src}
        alt=""
        loading="lazy"
        onError={() => setFailed(true)}
        className={`object-cover ${rounded} ${className}`}
      />
    );
  }
  return (
    <span className={`relative block overflow-hidden ${rounded} ${className}`}>
      {!loaded && (
        <img
          src={placeholderSrc}
          alt=""
          className="absolute inset-0 h-full w-full scale-105 object-cover blur-[6px]"
        />
      )}
      <img
        src={src}
        alt=""
        onLoad={() => setLoaded(true)}
        onError={() => setFailed(true)}
        className={`relative h-full w-full object-cover transition-opacity duration-300 ${
          loaded ? "opacity-100" : "opacity-0"
        }`}
      />
    </span>
  );
}

/** A tappable song row that plays `songs` starting at this one. */
export function SongRow({
  song,
  songs,
  position,
}: {
  song: Song;
  songs: Song[];
  position: number;
}) {
  const { playQueue, current, isPlaying, isStarred, toggleStar } = usePlayer();
  const active = current?.id === song.id;
  return (
    <div className="-mx-2 flex items-center gap-3 rounded-xl px-2 py-2 transition-colors hover:bg-black/[0.04] active:bg-black/[0.06] dark:hover:bg-white/[0.04] dark:active:bg-white/[0.07]">
      <button
        type="button"
        onClick={() => playQueue(songs, position)}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span className="relative h-11 w-11 shrink-0">
          <Cover coverArt={song.coverArt} size={80} className="h-full w-full" />
          {active && (
            <span className="absolute inset-0 grid place-items-center rounded-xl bg-black/45">
              <span className={`eq text-white ${isPlaying ? "" : "paused"}`}>
                <span />
                <span />
                <span />
              </span>
            </span>
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span
            className={`block truncate text-sm font-semibold ${
              active ? "text-wave-pink" : ""
            }`}
          >
            {song.title}
          </span>
          <span className="block truncate text-xs text-neutral-500 dark:text-neutral-400">
            {song.artist}
          </span>
        </span>
      </button>
      <button
        type="button"
        aria-label="like"
        onClick={() => toggleStar(song.id)}
        className={`grid h-8 w-8 shrink-0 place-items-center transition-transform active:scale-90 ${
          isStarred(song.id) ? "text-wave-pink" : "text-neutral-400 dark:text-neutral-500"
        }`}
      >
        <HeartIcon className="h-5 w-5" filled={isStarred(song.id)} />
      </button>
    </div>
  );
}

export function NowPlayingBar({ onOpen }: { onOpen: () => void }) {
  const { current, isPlaying, currentTime, duration, toggle, next } = usePlayer();
  if (!current) return null;
  const displayDuration = duration || current.duration || 0;
  const progress = displayDuration
    ? Math.min((currentTime / displayDuration) * 100, 100)
    : 0;
  return (
    <div className="relative border-b border-black/5 dark:border-white/5">
      <div className="absolute inset-x-4 top-0 h-0.5 overflow-hidden rounded-full bg-black/10 dark:bg-white/10">
        <div
          className="h-full rounded-full bg-gradient-to-r from-wave-orange to-wave-pink"
          style={{ width: `${progress}%` }}
        />
      </div>
      <div className="flex w-full items-center gap-3 px-4 py-2.5 text-left">
        <button
          type="button"
          onClick={onOpen}
          className="flex min-w-0 flex-1 items-center gap-3 text-left"
        >
          <Cover
            coverArt={current.coverArt}
            size={80}
            rounded="rounded-lg"
            className="h-10 w-10 shrink-0 shadow-md"
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-semibold">{current.title}</span>
            <span className="block truncate text-xs text-neutral-500 dark:text-neutral-400">
              {current.artist}
            </span>
          </span>
        </button>
        <button
          type="button"
          aria-label={isPlaying ? "pause" : "play"}
          onClick={toggle}
          className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-neutral-900 text-white shadow-md transition-transform active:scale-90 dark:bg-white dark:text-neutral-900"
        >
          {isPlaying ? (
            <PauseIcon className="h-5 w-5" />
          ) : (
            <PlayIcon className="ml-0.5 h-5 w-5" />
          )}
        </button>
        <button
          type="button"
          aria-label="next"
          onClick={next}
          className="grid h-9 w-9 shrink-0 place-items-center text-neutral-700 transition-transform active:scale-90 dark:text-neutral-200"
        >
          <NextIcon className="h-5 w-5" />
        </button>
      </div>
    </div>
  );
}

function QueueScreen({ onClose }: { onClose: () => void }) {
  const { queue, index, playQueue } = usePlayer();
  const upNext = queue.slice(index + 1);
  return (
    <div className="absolute inset-0 z-10 animate-fade-in bg-black/55 backdrop-blur-2xl">
      <div className="mx-auto flex h-full w-full max-w-md animate-slide-up flex-col px-6 pb-[max(env(safe-area-inset-bottom),1.5rem)] pt-[max(env(safe-area-inset-top),1.25rem)]">
        <header className="mb-5 flex items-center">
          <button
            type="button"
            onClick={onClose}
            aria-label="back"
            className="grid h-10 w-10 place-items-center rounded-full bg-white/10 text-white backdrop-blur transition-transform active:scale-90"
          >
            <ChevronLeftIcon className="h-6 w-6" />
          </button>
          <span className="flex-1 text-center text-xs font-bold uppercase tracking-[0.2em] text-white/60">
            Далее
          </span>
          <span className="h-10 w-10" />
        </header>
        {upNext.length === 0 ? (
          <p className="py-10 text-center text-sm text-white/50">Очередь пуста.</p>
        ) : (
          <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto pb-4">
            {upNext.map((song, offset) => (
              <button
                key={`${song.id}-${offset}`}
                type="button"
                onClick={() => playQueue(queue, index + 1 + offset)}
                className="-mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-xl px-2 py-2 text-left transition-colors active:bg-white/10"
              >
                <span className="grid h-5 w-7 shrink-0 place-items-center text-xs font-bold text-white/40">
                  {offset + 1}
                </span>
                <Cover
                  coverArt={song.coverArt}
                  size={80}
                  rounded="rounded-lg"
                  className="h-11 w-11 shrink-0"
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-semibold">{song.title}</span>
                  <span className="block truncate text-xs text-white/50">{song.artist}</span>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export function NowPlayingScreen({ onClose }: { onClose: () => void }) {
  const nav = useNav();
  const {
    current,
    isPlaying,
    currentTime,
    duration,
    toggle,
    next,
    prev,
    seek,
    isStarred,
    toggleStar,
    dislikeCurrent,
  } = usePlayer();
  const [queueOpen, setQueueOpen] = useState(false);
  if (!current) return null;
  const displayDuration = duration || current.duration || 0;
  const displayTime = displayDuration ? Math.min(currentTime, displayDuration) : currentTime;
  const progress = displayDuration
    ? Math.min((displayTime / displayDuration) * 100, 100)
    : 0;
  const openArtist = () => {
    onClose();
    if (current.artistId) {
      nav.push({ name: "artist", id: current.artistId, title: current.artist });
    } else {
      nav.push({ name: "artistLookup", title: current.artist });
    }
  };
  const openAlbum = () => {
    if (!current.albumId || !current.album) return;
    onClose();
    nav.push({ name: "album", id: current.albumId, title: current.album });
  };
  return (
    <div className="fixed inset-0 z-20 animate-fade-in overflow-hidden bg-neutral-950 text-white">
      {/* Ambient backdrop: the cover, blown up and blurred */}
      <div className="absolute inset-0">
        {/* size 80 matches the list rows, so this is already in cache; the
            heavy blur hides the low resolution */}
        <Cover
          coverArt={current.coverArt}
          size={80}
          rounded=""
          className="h-full w-full scale-125 opacity-50 blur-3xl saturate-150"
        />
        <div className="absolute inset-0 bg-gradient-to-b from-black/30 via-black/45 to-black/75" />
      </div>

      <div className="relative mx-auto flex h-full w-full max-w-md animate-slide-up flex-col px-6 pb-[max(env(safe-area-inset-bottom),1.5rem)] pt-[max(env(safe-area-inset-top),1.25rem)]">
        <header className="mb-5 flex items-center">
          <button
            type="button"
            onClick={onClose}
            aria-label="back"
            className="grid h-10 w-10 place-items-center rounded-full bg-white/10 text-white backdrop-blur transition-transform active:scale-90"
          >
            <ChevronLeftIcon className="h-6 w-6" />
          </button>
          <span className="flex-1 text-center text-xs font-bold uppercase tracking-[0.2em] text-white/60">
            Сейчас играет
          </span>
          <button
            type="button"
            onClick={() => setQueueOpen(true)}
            aria-label="queue"
            className="grid h-10 w-10 place-items-center rounded-full bg-white/10 text-white backdrop-blur transition-transform active:scale-90"
          >
            <QueueIcon className="h-5 w-5" />
          </button>
        </header>

        <Cover
          coverArt={current.coverArt}
          size={600}
          placeholderSize={80}
          rounded="rounded-xl"
          className="mx-auto aspect-square w-full max-w-xs shadow-[0_24px_60px_-12px_rgb(0_0_0/0.7)] ring-1 ring-white/10"
        />

        <div className="mt-7 flex items-center gap-3">
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-2xl font-extrabold tracking-tight">
              {current.title}
            </h2>
            <button
              type="button"
              onClick={openArtist}
              className="block max-w-full truncate text-left font-medium text-white/70 active:text-wave-pink"
            >
              {current.artist}
            </button>
            {current.album && (
              <button
                type="button"
                onClick={openAlbum}
                disabled={!current.albumId}
                className="block max-w-full truncate text-left text-sm text-white/50 enabled:active:text-wave-pink disabled:opacity-70"
              >
                {current.album}
              </button>
            )}
          </div>
          <button
            type="button"
            aria-label="like"
            onClick={() => toggleStar(current.id)}
            className={`transition-transform active:scale-90 ${
              isStarred(current.id) ? "text-wave-pink" : "text-white/60"
            }`}
          >
            <HeartIcon className="h-7 w-7" filled={isStarred(current.id)} />
          </button>
          <button
            type="button"
            aria-label="dislike"
            onClick={dislikeCurrent}
            className="text-white/60 transition-transform active:scale-90"
          >
            <BanIcon className="h-7 w-7" />
          </button>
        </div>

        <div className="mt-5">
          <input
            type="range"
            min={0}
            max={displayDuration}
            value={displayDuration ? displayTime : 0}
            onChange={(event) => seek(Number(event.target.value))}
            className="slider w-full"
            style={{ "--p": `${progress}%` } as CSSProperties}
          />
          <div className="flex justify-between text-xs font-medium text-white/50">
            <span>{formatTime(displayTime)}</span>
            <span>{formatTime(displayDuration)}</span>
          </div>
        </div>

        <div className="mt-4 flex items-center justify-center gap-9">
          <button
            type="button"
            aria-label="previous"
            onClick={prev}
            className="text-white/90 transition-transform active:scale-90"
          >
            <PrevIcon className="h-9 w-9" />
          </button>
          <button
            type="button"
            aria-label={isPlaying ? "pause" : "play"}
            onClick={toggle}
            className="grid h-18 w-18 place-items-center rounded-full bg-white text-neutral-950 shadow-[0_12px_30px_-6px_rgb(0_0_0/0.6)] transition-transform active:scale-95"
          >
            {isPlaying ? (
              <PauseIcon className="h-8 w-8" />
            ) : (
              <PlayIcon className="ml-1 h-8 w-8" />
            )}
          </button>
          <button
            type="button"
            aria-label="next"
            onClick={next}
            className="text-white/90 transition-transform active:scale-90"
          >
            <NextIcon className="h-9 w-9" />
          </button>
        </div>

        {/* Free space below the controls — reserved for lyrics & friends. */}
        <div className="flex-1" />
      </div>

      {queueOpen && <QueueScreen onClose={() => setQueueOpen(false)} />}
    </div>
  );
}
