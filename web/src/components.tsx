import { useEffect, useState } from "react";
import {
  BanIcon,
  CastIcon,
  ChevronLeftIcon,
  DownloadDoneIcon,
  DownloadIcon,
  HeartIcon,
  LyricsIcon,
  MusicNoteIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
  QueueIcon,
  RepeatIcon,
  RepeatOneIcon,
  ShuffleIcon,
} from "./icons";
import { getFriends, getLyrics } from "./api";
import { useDownloads } from "./downloads";
import { useNav } from "./nav";
import { formatTime, usePlayer } from "./player";
import type { FriendActivity, LyricsResult, Song } from "./types";

function Spinner({ className = "h-5 w-5" }: { className?: string }) {
  return (
    <span
      className={`inline-block animate-spin rounded-full border-2 border-wave-pink/30 border-t-wave-pink ${className}`}
    />
  );
}

/** Per-track offline download toggle: download → spinner → green check. */
export function DownloadButton({
  song,
  className = "",
  size = "h-5 w-5",
}: {
  song: Song;
  className?: string;
  size?: string;
}) {
  const { isDownloaded, isDownloading, toggle } = useDownloads();
  const done = isDownloaded(song.id);
  const busy = isDownloading(song.id);
  return (
    <button
      type="button"
      aria-label={done ? "remove download" : "download"}
      onClick={() => toggle(song)}
      disabled={busy}
      className={`grid place-items-center transition-transform active:scale-90 ${
        done ? "text-green-500" : "text-neutral-400 dark:text-neutral-500"
      } ${className}`}
    >
      {busy ? (
        <Spinner className={size} />
      ) : done ? (
        <DownloadDoneIcon className={size} />
      ) : (
        <DownloadIcon className={size} />
      )}
    </button>
  );
}

/** Album/playlist "download everything" pill with progress + remove-all. */
export function DownloadAllButton({ songs }: { songs: Song[] }) {
  const { isDownloading, downloadAlbum, downloadedCount, remove } = useDownloads();
  const ids = songs.map((song) => song.id);
  const done = downloadedCount(ids);
  const total = songs.length;
  const allDone = total > 0 && done === total;
  const busy = songs.some((song) => isDownloading(song.id));
  return (
    <button
      type="button"
      disabled={total === 0 || busy}
      onClick={() => (allDone ? ids.forEach((id) => remove(id)) : downloadAlbum(songs))}
      className={`inline-flex items-center gap-2 rounded-full border px-4 py-2.5 font-bold transition active:scale-95 disabled:opacity-60 ${
        allDone
          ? "border-green-500/30 bg-green-500/10 text-green-600 dark:text-green-400"
          : "border-black/10 bg-black/[0.03] text-neutral-700 dark:border-white/15 dark:bg-white/5 dark:text-neutral-200"
      }`}
    >
      {busy ? (
        <Spinner className="h-5 w-5" />
      ) : allDone ? (
        <DownloadDoneIcon className="h-5 w-5" />
      ) : (
        <DownloadIcon className="h-5 w-5" />
      )}
      <span>{busy ? `${done}/${total}` : allDone ? "Скачано" : "Скачать"}</span>
    </button>
  );
}

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

function SeekBar({
  duration,
  currentTime,
  progress,
  seek,
}: {
  duration: number;
  currentTime: number;
  progress: number;
  seek: (seconds: number) => void;
}) {
  const safeProgress = Math.min(Math.max(progress, 0), 100);
  const disabled = duration <= 0;
  return (
    <div className="relative h-8">
      <div className="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 overflow-hidden bg-[#e9e2d4]/15">
        <div
          className="h-full origin-left bg-gradient-to-r from-[#7a0c1f] to-wave-pink transition-transform duration-200 ease-linear"
          style={{ transform: `scaleX(${safeProgress / 100})` }}
        />
      </div>
      <div
        className="absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rotate-45 rounded-[3px] bg-[#e9e2d4] shadow-[0_0_10px_rgb(196_30_58/0.75)] transition-[left] duration-200 ease-linear"
        style={{ left: `${safeProgress}%` }}
      />
      <input
        type="range"
        min={0}
        max={duration || 1}
        value={disabled ? 0 : currentTime}
        disabled={disabled}
        onChange={(event) => seek(Number(event.target.value))}
        aria-label="seek"
        className="absolute inset-x-0 top-1/2 h-8 -translate-y-1/2 cursor-pointer opacity-0 disabled:cursor-default"
      />
    </div>
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
            {song.provider === "yandex" ? (
              <span className="mr-1 rounded-full bg-wave-pink/15 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-[0.12em] text-wave-pink">
                Yandex
              </span>
            ) : null}
            {song.artist}
          </span>
        </span>
      </button>
      <DownloadButton song={song} className="h-8 w-8 shrink-0" />
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
          className="h-full rounded-full bg-gradient-to-r from-wave-orange to-wave-pink transition-[width] duration-500 ease-out"
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
      <div className="mx-auto flex h-full w-full max-w-md animate-slide-up flex-col px-6 pb-[max(env(safe-area-inset-bottom),1.5rem)] pt-[max(env(safe-area-inset-top),1.25rem)] md:max-w-3xl">
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
                  <span className="block truncate text-xs text-white/50">
                    {song.provider === "yandex" ? (
                      <span className="mr-1 rounded-full bg-wave-pink/15 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-[0.12em] text-wave-pink">
                        Yandex
                      </span>
                    ) : null}
                    {song.artist}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function activeLyricsIndex(lyrics: LyricsResult | null, currentTime: number): number {
  if (!lyrics?.synced) return -1;
  const nowMs = currentTime * 1000 + 150;
  let active = -1;
  for (let i = 0; i < lyrics.lines.length; i += 1) {
    const start = lyrics.lines[i].start;
    if (start === undefined || start > nowMs) break;
    active = i;
  }
  return active;
}

function LyricsScreen({
  current,
  onClose,
  lyrics,
  loading,
  currentTime,
  seek,
}: {
  current: Song;
  onClose: () => void;
  lyrics: LyricsResult | null;
  loading: boolean;
  currentTime: number;
  seek: (seconds: number) => void;
}) {
  const active = activeLyricsIndex(lyrics, currentTime);
  return (
    <div className="fixed inset-0 z-30 animate-slide-up overflow-hidden bg-neutral-950 text-white">
      <div className="absolute inset-0">
        <Cover
          coverArt={current.coverArt}
          size={80}
          rounded=""
          className="h-full w-full scale-125 opacity-45 blur-3xl saturate-150"
        />
        <div className="absolute inset-0 bg-gradient-to-b from-black/50 via-black/60 to-black/85" />
      </div>

      <div className="relative mx-auto flex h-full w-full max-w-md flex-col px-6 pb-[max(env(safe-area-inset-bottom),1.5rem)] pt-[max(env(safe-area-inset-top),1.25rem)] md:max-w-3xl">
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
            Текст песни
          </span>
          <span className="h-10 w-10" />
        </header>

        <div className="mb-6 flex items-center gap-3">
          <Cover
            coverArt={current.coverArt}
            size={96}
            rounded="rounded-xl"
            className="h-14 w-14 shrink-0 shadow-lg ring-1 ring-white/10"
          />
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-xl font-extrabold tracking-tight">{current.title}</h2>
            <p className="truncate text-sm font-medium text-white/60">{current.artist}</p>
          </div>
          {lyrics?.synced && (
            <span className="rounded-full border border-wave-pink/30 bg-wave-pink/10 px-2.5 py-1 text-[0.65rem] font-bold uppercase tracking-[0.16em] text-wave-pink/90">
              live
            </span>
          )}
        </div>

        {loading ? (
          <div className="grid min-h-0 flex-1 place-items-center text-center">
            <p className="text-base font-bold text-white/50">Ищем текст...</p>
          </div>
        ) : !lyrics ? (
          <div className="grid min-h-0 flex-1 place-items-center text-center">
            <div>
              <LyricsIcon className="mx-auto mb-4 h-10 w-10 text-white/30" />
              <p className="text-lg font-extrabold text-white/60">Текста пока нет.</p>
              <p className="mt-2 text-sm font-medium text-white/35">
                Если текст есть у провайдера, он появится здесь.
              </p>
            </div>
          </div>
        ) : (
          <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto pb-8 pr-1">
            {lyrics.lines.map((line, index) => {
              const isActive = index === active;
              const canSeek = lyrics.synced && line.start !== undefined;
              return (
                <button
                  key={`${line.start ?? index}-${line.value}`}
                  type="button"
                  disabled={!canSeek}
                  onClick={() => {
                    if (line.start !== undefined) seek(line.start / 1000);
                  }}
                  className={`block w-full rounded-xl px-2 py-2.5 text-left text-2xl font-extrabold leading-tight transition-colors ${
                    isActive
                      ? "text-white"
                      : "text-white/40 enabled:active:text-wave-pink"
                  }`}
                >
                  {line.value}
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

export function NowPlayingScreen({ onClose }: { onClose: () => void }) {
  const nav = useNav();
  const {
    session,
    current,
    isPlaying,
    currentTime,
    duration,
    repeat,
    shuffle,
    toggle,
    next,
    prev,
    seek,
    cycleRepeat,
    toggleShuffle,
    isStarred,
    toggleStar,
    dislikeCurrent,
  } = usePlayer();
  const [queueOpen, setQueueOpen] = useState(false);
  const [lyricsOpen, setLyricsOpen] = useState(false);
  const [lyrics, setLyrics] = useState<LyricsResult | null>(null);
  const [lyricsLoading, setLyricsLoading] = useState(false);
  useEffect(() => {
    if (!current) {
      setLyrics(null);
      setLyricsLoading(false);
      return;
    }
    let cancelled = false;
    setLyrics(null);
    setLyricsLoading(true);
    getLyrics(session, current.id)
      .then((result) => {
        if (!cancelled) setLyrics(result);
      })
      .catch(() => {
        if (!cancelled) setLyrics(null);
      })
      .finally(() => {
        if (!cancelled) setLyricsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [current?.id, session]);
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

      <div className="relative mx-auto flex h-full w-full max-w-md animate-slide-up flex-col px-6 pb-[max(env(safe-area-inset-bottom),1.5rem)] pt-[max(env(safe-area-inset-top),1.25rem)] md:max-w-5xl md:px-8 lg:px-10">
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
          className="mx-auto aspect-square w-full max-w-xs shadow-[0_24px_60px_-12px_rgb(0_0_0/0.7)] ring-1 ring-white/10 md:max-w-sm lg:max-w-md"
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
              {current.provider === "yandex" ? (
                <span className="mr-2 rounded-full bg-wave-pink/20 px-2 py-0.5 text-[10px] font-bold uppercase tracking-[0.12em] text-wave-pink">
                  Yandex
                </span>
              ) : null}
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
          <DownloadButton song={current} className="h-7 w-7" size="h-7 w-7" />
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
          <SeekBar
            duration={displayDuration}
            currentTime={displayTime}
            progress={progress}
            seek={seek}
          />
          <div className="flex justify-between text-xs font-medium text-white/50">
            <span>{formatTime(displayTime)}</span>
            <span>{formatTime(displayDuration)}</span>
          </div>
        </div>

        <div className="mt-4 flex items-center justify-center gap-6">
          <button
            type="button"
            aria-label="shuffle"
            aria-pressed={shuffle}
            onClick={toggleShuffle}
            className={`transition-transform active:scale-90 ${
              shuffle ? "text-wave-pink" : "text-white/60"
            }`}
          >
            <ShuffleIcon className="h-6 w-6" />
          </button>
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
          <button
            type="button"
            aria-label={`repeat ${repeat}`}
            onClick={cycleRepeat}
            className={`transition-transform active:scale-90 ${
              repeat === "off" ? "text-white/60" : "text-wave-pink"
            }`}
          >
            {repeat === "one" ? (
              <RepeatOneIcon className="h-6 w-6" />
            ) : (
              <RepeatIcon className="h-6 w-6" />
            )}
          </button>
        </div>

        <div className="mt-5 flex justify-center">
          <button
            type="button"
            onClick={() => setLyricsOpen(true)}
            className="inline-flex h-11 items-center gap-2 rounded-full bg-white/10 px-4 text-sm font-bold text-white/75 ring-1 ring-white/10 backdrop-blur transition active:scale-95 active:text-white"
          >
            <LyricsIcon className="h-5 w-5" />
            <span>{lyricsLoading ? "Ищем текст" : "Текст"}</span>
          </button>
        </div>

        <div className="min-h-4 flex-1" />
      </div>

      {queueOpen && <QueueScreen onClose={() => setQueueOpen(false)} />}
      {lyricsOpen && current && (
        <LyricsScreen
          current={current}
          onClose={() => setLyricsOpen(false)}
          lyrics={lyrics}
          loading={lyricsLoading}
          currentTime={displayTime}
          seek={seek}
        />
      )}
    </div>
  );
}

function timeAgo(epochSecs: number): string {
  if (!epochSecs) return "";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - epochSecs);
  if (diff < 60) return "только что";
  if (diff < 3600) return `${Math.floor(diff / 60)} мин назад`;
  if (diff < 86_400) return `${Math.floor(diff / 3600)} ч назад`;
  return `${Math.floor(diff / 86_400)} дн назад`;
}

/** Sidebar toggle: hand playback to the Discord bot (Vivaldi) and back. While
 *  connected, the app is a remote — the playbar drives the bot. */
export function DiscordConnectToggle() {
  const { remoteOn, remoteConnected, connectRemote, disconnectRemote } = usePlayer();
  const label = remoteOn
    ? remoteConnected
      ? "Vivaldi · подключено"
      : "Подключаюсь…"
    : "Слушать в Discord";
  return (
    <button
      type="button"
      onClick={() => (remoteOn ? disconnectRemote() : connectRemote())}
      className={`flex w-full items-center gap-3 rounded-xl border px-3 py-2.5 text-left text-sm font-bold transition active:scale-[0.98] ${
        remoteOn
          ? "border-wave-pink/30 bg-wave-pink/10 text-wave-pink"
          : "border-white/10 text-neutral-400 hover:bg-white/[0.04] hover:text-neutral-200"
      }`}
    >
      <CastIcon className="h-5 w-5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {remoteOn && (
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${
            remoteConnected ? "bg-green-400" : "animate-pulse bg-amber-400"
          }`}
        />
      )}
    </button>
  );
}

/** Right-side Friend Activity feed: what everyone else is listening to. Tap a
 *  row to play that track. (For now every account on the instance is a friend.) */
export function FriendsPanel() {
  const { session, playQueue } = usePlayer();
  const [friends, setFriends] = useState<FriendActivity[]>([]);
  useEffect(() => {
    let active = true;
    const load = () =>
      getFriends(session)
        .then((list) => {
          if (active) setFriends(list);
        })
        .catch(() => undefined);
    load();
    const id = window.setInterval(load, 45_000);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [session]);

  return (
    <aside className="hidden border-l border-wave-pink/10 px-5 py-6 xl:block">
      <div className="sticky top-6">
        <h2 className="mb-4 text-xs font-bold uppercase tracking-[0.2em] text-neutral-500">
          Чем заняты друзья
        </h2>
        {friends.length === 0 ? (
          <p className="rounded-xl border border-white/10 bg-white/[0.035] p-4 text-sm font-semibold text-neutral-500">
            Пока тихо. Когда друзья что-то слушают, это появится здесь.
          </p>
        ) : (
          <div className="space-y-1">
            {friends.map((friend) => (
              <button
                key={friend.username}
                type="button"
                onClick={() => playQueue([friend.song], 0)}
                className="flex w-full items-center gap-3 rounded-xl p-2 text-left transition hover:bg-white/[0.05] active:scale-[0.99]"
              >
                <span className="relative shrink-0">
                  <Cover
                    coverArt={friend.song.coverArt}
                    size={80}
                    rounded="rounded-lg"
                    className="h-11 w-11 ring-1 ring-white/10"
                  />
                  <span className="absolute -bottom-1 -right-1 grid h-5 w-5 place-items-center rounded-full bg-gradient-to-br from-wave-orange to-wave-violet text-[10px] font-bold text-white ring-2 ring-[#0d070b]">
                    {friend.username.slice(0, 1).toUpperCase()}
                  </span>
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-bold text-[#f3ecdd]">
                    {friend.username}
                  </span>
                  <span className="block truncate text-xs text-neutral-400">
                    {friend.song.title} · {friend.song.artist}
                  </span>
                  <span className="block truncate text-[11px] text-neutral-500">
                    {timeAgo(friend.updatedAt)}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}

/** Persistent desktop bottom playbar (Spotify/Flutter style): track + controls
 *  + progress. Tap the cover/title to open the full-screen player. */
export function PlayBar({ onOpen }: { onOpen: () => void }) {
  const {
    current,
    isPlaying,
    currentTime,
    duration,
    repeat,
    shuffle,
    remoteOn,
    toggle,
    next,
    prev,
    seek,
    cycleRepeat,
    toggleShuffle,
    isStarred,
    toggleStar,
  } = usePlayer();
  if (!current) return null;
  const displayDuration = duration || current.duration || 0;
  const displayTime = displayDuration ? Math.min(currentTime, displayDuration) : currentTime;
  const progress = displayDuration ? Math.min((displayTime / displayDuration) * 100, 100) : 0;
  return (
    <div className="hidden border-t border-wave-pink/15 bg-[#0d070b]/95 px-4 py-2.5 text-white backdrop-blur-2xl lg:block">
      <div className="mx-auto grid max-w-[1500px] grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-4">
        {/* Left: track */}
        <div className="flex min-w-0 items-center gap-3">
          <button type="button" onClick={onOpen} aria-label="open player" className="shrink-0">
            <Cover
              coverArt={current.coverArt}
              size={120}
              rounded="rounded-lg"
              className="h-14 w-14 shadow-md ring-1 ring-white/10"
            />
          </button>
          <button type="button" onClick={onOpen} className="min-w-0 flex-1 text-left">
            <span className="block truncate text-sm font-bold text-[#f3ecdd]">
              {current.title}
            </span>
            <span className="block truncate text-xs font-semibold text-neutral-400">
              {current.artist}
            </span>
          </button>
          {remoteOn && (
            <span className="hidden shrink-0 items-center gap-1 rounded-full bg-wave-pink/15 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-wave-pink xl:inline-flex">
              <CastIcon className="h-3 w-3" /> Discord
            </span>
          )}
          <button
            type="button"
            aria-label="like"
            onClick={() => toggleStar(current.id)}
            className={`shrink-0 transition-transform active:scale-90 ${
              isStarred(current.id) ? "text-wave-pink" : "text-neutral-400"
            }`}
          >
            <HeartIcon className="h-5 w-5" filled={isStarred(current.id)} />
          </button>
        </div>

        {/* Center: controls + progress */}
        <div className="flex w-[min(42vw,560px)] flex-col items-center gap-1">
          <div className="flex items-center gap-5">
            <button
              type="button"
              aria-label="shuffle"
              aria-pressed={shuffle}
              onClick={toggleShuffle}
              className={`transition active:scale-90 ${
                shuffle ? "text-wave-pink" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              <ShuffleIcon className="h-5 w-5" />
            </button>
            <button
              type="button"
              aria-label="previous"
              onClick={prev}
              className="text-neutral-200 transition active:scale-90 hover:text-white"
            >
              <PrevIcon className="h-6 w-6" />
            </button>
            <button
              type="button"
              aria-label={isPlaying ? "pause" : "play"}
              onClick={toggle}
              className="grid h-10 w-10 place-items-center rounded-full bg-[#f3ecdd] text-neutral-950 shadow-md transition active:scale-95"
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
              className="text-neutral-200 transition active:scale-90 hover:text-white"
            >
              <NextIcon className="h-6 w-6" />
            </button>
            <button
              type="button"
              aria-label={`repeat ${repeat}`}
              onClick={cycleRepeat}
              className={`transition active:scale-90 ${
                repeat === "off" ? "text-neutral-400 hover:text-neutral-200" : "text-wave-pink"
              }`}
            >
              {repeat === "one" ? (
                <RepeatOneIcon className="h-5 w-5" />
              ) : (
                <RepeatIcon className="h-5 w-5" />
              )}
            </button>
          </div>
          <div className="flex w-full items-center gap-2 text-[11px] font-semibold text-neutral-500">
            <span className="w-9 text-right tabular-nums">{formatTime(displayTime)}</span>
            <div className="flex-1">
              <SeekBar
                duration={displayDuration}
                currentTime={displayTime}
                progress={progress}
                seek={seek}
              />
            </div>
            <span className="w-9 tabular-nums">{formatTime(displayDuration)}</span>
          </div>
        </div>

        {/* Right: download + open */}
        <div className="flex items-center justify-end gap-2">
          <DownloadButton song={current} className="h-9 w-9" />
          <button
            type="button"
            aria-label="now playing"
            onClick={onOpen}
            className="grid h-9 w-9 place-items-center rounded-full text-neutral-300 transition hover:bg-white/[0.06] hover:text-white"
          >
            <QueueIcon className="h-5 w-5" />
          </button>
        </div>
      </div>
    </div>
  );
}
