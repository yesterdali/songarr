import {
  CastIcon,
  HeartIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
  QueueIcon,
  RepeatIcon,
  RepeatOneIcon,
  ShuffleIcon,
  VolumeIcon,
} from "./icons";
import { formatTime, usePlayer } from "./player";
import { Cover, DownloadButton, SeekBar } from "./components";

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
    volume,
    muted,
    toggle,
    next,
    prev,
    seek,
    cycleRepeat,
    toggleShuffle,
    setVolume,
    toggleMute,
    isStarred,
    toggleStar,
  } = usePlayer();
  if (!current) return null;
  const displayDuration = duration || current.duration || 0;
  const displayTime = displayDuration ? Math.min(currentTime, displayDuration) : currentTime;
  const progress = displayDuration ? Math.min((displayTime / displayDuration) * 100, 100) : 0;
  const effectiveVolume = muted ? 0 : volume;
  return (
    <div className="hidden border-t border-wave-pink/15 bg-[#0d070b]/95 px-4 py-2.5 text-white backdrop-blur-2xl lg:block">
      <div className="mx-auto grid max-w-[1500px] grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-4">
        {/* Left: track */}
        <div className="flex min-w-0 items-center gap-3">
          <button type="button" onClick={onOpen} aria-label="open player" className="shrink-0">
            <Cover
              coverArt={current.coverArt}
              downloadId={current.id}
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
          <div className="hidden items-center gap-2 rounded-full bg-white/[0.04] px-2.5 py-1.5 ring-1 ring-white/10 xl:flex">
            <button
              type="button"
              aria-label={muted || volume === 0 ? "unmute" : "mute"}
              onClick={toggleMute}
              className="grid h-7 w-7 place-items-center rounded-full text-neutral-300 transition hover:bg-white/[0.06] hover:text-white active:scale-95"
            >
              <VolumeIcon className="h-5 w-5" muted={muted || volume === 0} />
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={effectiveVolume}
              onChange={(event) => setVolume(Number(event.target.value))}
              aria-label="volume"
              className="h-1.5 w-24 accent-wave-pink"
            />
            <span className="w-8 text-right text-[11px] font-bold tabular-nums text-neutral-500">
              {Math.round(effectiveVolume * 100)}
            </span>
          </div>
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
