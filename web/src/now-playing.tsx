import { useEffect, useState } from "react";
import {
  BanIcon,
  ChevronLeftIcon,
  HeartIcon,
  LyricsIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
  QueueIcon,
  RepeatIcon,
  RepeatOneIcon,
  ShuffleIcon,
} from "./icons";
import { getLyrics } from "./api";
import { useNav } from "./nav";
import { formatTime, usePlayer } from "./player";
import { getStreamQuality, qualityLabel } from "./quality";
import { reasonLabel } from "./reasons";
import type { LyricsResult, Song } from "./types";
import { Cover, DownloadButton, SeekBar } from "./components";

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

function QueueSongRow({
  song,
  offset,
  onClick,
}: {
  song: Song;
  offset: number;
  onClick: () => void;
}) {
  const reason = reasonLabel(song);
  return (
    <button
      type="button"
      onClick={onClick}
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
        {reason && (
          <span className="block truncate text-[11px] font-semibold text-wave-pink/70">
            {reason}
          </span>
        )}
      </span>
    </button>
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
              <QueueSongRow
                key={`${song.id}-${offset}`}
                song={song}
                offset={offset}
                onClick={() => playQueue(queue, index + 1 + offset)}
              />
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
    moreLikeCurrent,
  } = usePlayer();
  const [moreBusy, setMoreBusy] = useState(false);
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
  const streamQuality = getStreamQuality();
  const qualityReadout =
    streamQuality === "lossless" && current.provider ? "Лучшее доступное" : qualityLabel(streamQuality);
  const currentReason = reasonLabel(current);
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
            <p className="mt-2 text-[11px] font-bold uppercase tracking-[0.16em] text-white/35">
              {qualityReadout}
            </p>
            {currentReason && (
              <p className="mt-1 truncate text-xs font-semibold text-wave-pink/75">
                {currentReason}
              </p>
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

        <div className="mt-5 flex flex-wrap justify-center gap-2">
          <button
            type="button"
            disabled={moreBusy}
            onClick={() => {
              setMoreBusy(true);
              moreLikeCurrent().finally(() => setMoreBusy(false)).catch(() => undefined);
            }}
            className="inline-flex h-11 items-center gap-2 rounded-full bg-white/10 px-4 text-sm font-bold text-white/75 ring-1 ring-white/10 backdrop-blur transition active:scale-95 active:text-white disabled:opacity-60"
          >
            <ShuffleIcon className="h-5 w-5" />
            <span>{moreBusy ? "Ищу" : "Похожее"}</span>
          </button>
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

