import { useEffect, useState, type ComponentType, type ReactNode } from "react";
import {
  CastIcon,
  DownloadDoneIcon,
  DownloadIcon,
  HeartIcon,
  MusicNoteIcon,
  QueueIcon,
  SettingsIcon,
} from "./icons";
import { avatarUrl, getFriends, getProfile } from "./api";
import { useDownloads } from "./downloads";
import { useI18n } from "./i18n";
import { useNav } from "./nav";
import { usePlayer } from "./player";
import { reasonLabel } from "./reasons";
import type { FriendActivity, Profile, Song } from "./types";

/** A user's avatar (image), falling back to their initial if none is set. */
export function Avatar({
  username,
  name,
  className = "h-9 w-9",
  textClass = "text-sm",
  bust,
}: {
  username: string;
  name?: string;
  className?: string;
  textClass?: string;
  bust?: number;
}) {
  const { session } = usePlayer();
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [username, bust]);
  const initial = (name || username).slice(0, 1).toUpperCase();
  if (failed) {
    return (
      <span
        className={`grid shrink-0 place-items-center rounded-full bg-gradient-to-br from-wave-orange to-wave-violet font-bold text-white ${className} ${textClass}`}
      >
        {initial}
      </span>
    );
  }
  const src = bust ? `${avatarUrl(session, username)}&v=${bust}` : avatarUrl(session, username);
  return (
    <img
      src={src}
      alt=""
      onError={() => setFailed(true)}
      className={`shrink-0 rounded-full object-cover ${className}`}
    />
  );
}

/** Sidebar account chip: avatar + display name, opens Settings. */
export function AccountButton() {
  const { session } = usePlayer();
  const nav = useNav();
  const [profile, setProfile] = useState<Profile | null>(null);
  useEffect(() => {
    let active = true;
    getProfile(session)
      .then((p) => {
        if (active) setProfile(p);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [session]);
  const name = profile?.displayName || session.username;
  return (
    <button
      type="button"
      onClick={() => nav.push({ name: "settings" })}
      className="flex w-full items-center gap-3 rounded-xl border border-white/10 bg-white/[0.04] p-3 text-left transition hover:bg-white/[0.06]"
    >
      <Avatar username={session.username} name={name} />
      <span className="min-w-0 flex-1 truncate text-sm font-bold">{name}</span>
      <SettingsIcon className="h-4 w-4 shrink-0 text-neutral-500" />
    </button>
  );
}

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
  const { t } = useI18n();
  const { isDownloaded, isDownloading, toggle, downloadError } = useDownloads();
  const done = isDownloaded(song.id);
  const busy = isDownloading(song.id);
  const error = downloadError(song.id);
  return (
    <button
      type="button"
      aria-label={done ? t("removeDownload") : t("download")}
      title={error ?? (done ? t("removeDownload") : t("download"))}
      onClick={() => toggle(song)}
      disabled={busy}
      className={`grid place-items-center transition-transform active:scale-90 ${
        error
          ? "text-red-500"
          : done
            ? "text-green-500"
            : "text-neutral-400 dark:text-neutral-500"
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
  const { t } = useI18n();
  const { isDownloading, downloadAlbum, downloadedCount, remove } = useDownloads();
  const ids = songs.map((song) => song.id);
  const done = downloadedCount(ids);
  const total = songs.length;
  const allDone = total > 0 && done === total;
  const partial = done > 0 && !allDone;
  const busy = songs.some((song) => isDownloading(song.id));
  return (
    <button
      type="button"
      disabled={total === 0 || busy}
      onClick={() => (allDone ? ids.forEach((id) => remove(id)) : downloadAlbum(songs))}
      className={`inline-flex items-center gap-2 rounded-full border px-4 py-2.5 font-bold transition active:scale-95 disabled:opacity-60 ${
        allDone
          ? "border-green-500/30 bg-green-500/10 text-green-600 dark:text-green-400"
          : partial
            ? "border-wave-pink/30 bg-wave-pink/10 text-wave-pink"
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
      <span>
        {busy || partial
          ? t("partialDownload", { done, total })
          : allDone
            ? t("downloadedDone")
            : t("download")}
      </span>
    </button>
  );
}

/** A single shimmering placeholder block. */
export function Skeleton({ className = "" }: { className?: string }) {
  return <div className={`animate-pulse rounded-xl bg-white/[0.06] ${className}`} />;
}

/** Skeleton matching the album/cover grid layout (covers + two text lines). */
export function SkeletonCardGrid({ count = 10 }: { count?: number }) {
  return (
    <div className="grid grid-cols-2 gap-4 md:grid-cols-4 md:gap-5 lg:grid-cols-5">
      {Array.from({ length: count }).map((_, i) => (
        <div key={i}>
          <Skeleton className="aspect-square w-full rounded-lg" />
          <Skeleton className="mt-2 h-3.5 w-3/4 rounded" />
          <Skeleton className="mt-1.5 h-3 w-1/2 rounded" />
        </div>
      ))}
    </div>
  );
}

/** Skeleton matching list rows (square thumb + two text lines). */
export function SkeletonRows({ count = 7 }: { count?: number }) {
  return (
    <div>
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="-mx-2 flex items-center gap-3 px-2 py-2">
          <Skeleton className="h-11 w-11 shrink-0" />
          <div className="min-w-0 flex-1">
            <Skeleton className="h-3.5 w-1/2 rounded" />
            <Skeleton className="mt-1.5 h-3 w-1/3 rounded" />
          </div>
        </div>
      ))}
    </div>
  );
}

/** Consistent empty state: icon-in-circle + copy. */
export function EmptyState({
  icon: Icon,
  title,
  hint,
}: {
  icon: ComponentType<{ className?: string }>;
  title: ReactNode;
  hint?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-3 py-12 text-center">
      <span className="grid h-14 w-14 place-items-center rounded-full bg-wave-pink/10 text-wave-pink">
        <Icon className="h-7 w-7" />
      </span>
      <p className="max-w-60 text-sm text-neutral-500 dark:text-neutral-400">{title}</p>
      {hint && <p className="max-w-64 text-xs text-neutral-600 dark:text-neutral-500">{hint}</p>}
    </div>
  );
}

/** Single "pick one" control used everywhere (quality, language, album sort):
 *  bordered chips with a pink-tint active state. The caller controls the grid
 *  via `className` so 3-, 4- and 5-option groups all stay tidy. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  className = "flex flex-wrap gap-2",
}: {
  value: T;
  options: readonly { value: T; label: ReactNode }[];
  onChange: (value: T) => void;
  className?: string;
}) {
  return (
    <div className={className}>
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(opt.value)}
            className={`rounded-xl border px-3 py-2.5 text-sm font-bold transition active:scale-95 ${
              active
                ? "border-wave-pink/40 bg-wave-pink/10 text-wave-pink"
                : "border-black/10 text-neutral-600 hover:bg-black/[0.04] dark:border-white/10 dark:text-neutral-300 dark:hover:bg-white/[0.04]"
            }`}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

export function Cover({
  coverArt,
  size = 200,
  className = "",
  rounded = "rounded-xl",
  placeholderSize,
  downloadId,
}: {
  coverArt?: string;
  size?: number;
  className?: string;
  rounded?: string;
  /** Show this (usually browser-cached) smaller size blurred-up while the
   *  full-size image is still downloading. */
  placeholderSize?: number;
  /** Optional downloaded song id; when offline, render its cached cover blob. */
  downloadId?: string;
}) {
  const { cover } = usePlayer();
  const { getCoverUrl } = useDownloads();
  const [failed, setFailed] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [localSrc, setLocalSrc] = useState<string | null>(null);
  const src = cover(coverArt, size);
  const placeholderSrc = placeholderSize ? cover(coverArt, placeholderSize) : undefined;
  useEffect(() => {
    setFailed(false);
    setLoaded(false);
    setLocalSrc(null);
    let cancelled = false;
    if (downloadId) {
      getCoverUrl(downloadId).then((url) => {
        if (!cancelled) setLocalSrc(url);
      });
    }
    return () => {
      cancelled = true;
    };
  }, [coverArt, downloadId, getCoverUrl]);
  const effectiveSrc = localSrc ?? src;
  if (!effectiveSrc || failed) {
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
        src={effectiveSrc}
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
        src={effectiveSrc}
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

export function SeekBar({
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
  const { t } = useI18n();
  const active = current?.id === song.id;
  const reason = reasonLabel(song, t);
  return (
    <div className="-mx-2 flex items-center gap-3 rounded-xl px-2 py-2 transition-colors hover:bg-black/[0.04] active:bg-black/[0.06] dark:hover:bg-white/[0.04] dark:active:bg-white/[0.07]">
      <button
        type="button"
        onClick={() => playQueue(songs, position)}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span className="relative h-11 w-11 shrink-0">
          <Cover coverArt={song.coverArt} downloadId={song.id} size={80} className="h-full w-full" />
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
          {reason && (
            <span className="block truncate text-[11px] font-semibold text-wave-pink/75">
              {reason}
            </span>
          )}
        </span>
      </button>
      <DownloadButton song={song} className="h-8 w-8 shrink-0" />
      <button
        type="button"
        aria-label={t("liked")}
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

function timeAgo(epochSecs: number, t: ReturnType<typeof useI18n>["t"]): string {
  if (!epochSecs) return "";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - epochSecs);
  if (diff < 60) return t("justNow");
  if (diff < 3600) return t("minutesAgo", { count: Math.floor(diff / 60) });
  if (diff < 86_400) return t("hoursAgo", { count: Math.floor(diff / 3600) });
  return t("daysAgo", { count: Math.floor(diff / 86_400) });
}

/** Sidebar toggle: hand playback to the Discord bot (Vivaldi) and back. While
 *  connected, the app is a remote — the playbar drives the bot. */
export function DiscordConnectToggle() {
  const { t } = useI18n();
  const { remoteOn, remoteConnected, remoteBusy, connectRemote, disconnectRemote } = usePlayer();
  const label = !remoteOn
    ? t("discordListen")
    : remoteBusy
      ? t("discordBusy")
      : remoteConnected
        ? t("discordConnected")
        : t("discordConnecting");
  const accent = remoteBusy
    ? "border-amber-500/40 bg-amber-500/10 text-amber-400"
    : remoteOn
      ? "border-wave-pink/30 bg-wave-pink/10 text-wave-pink"
      : "border-white/10 text-neutral-400 hover:bg-white/[0.04] hover:text-neutral-200";
  return (
    <button
      type="button"
      onClick={() => (remoteOn ? disconnectRemote() : connectRemote())}
      className={`flex w-full items-center gap-3 rounded-xl border px-3 py-2.5 text-left text-sm font-bold transition active:scale-[0.98] ${accent}`}
    >
      <CastIcon className="h-5 w-5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {remoteOn && (
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${
            remoteBusy
              ? "bg-amber-400"
              : remoteConnected
                ? "bg-green-400"
                : "animate-pulse bg-amber-400"
          }`}
        />
      )}
    </button>
  );
}

/** Listen Together room controls: create/join/leave plus the tiny phase-C
 *  surface for reactions and lightweight chat. */
export function ListenTogetherPanel() {
  const { t } = useI18n();
  const {
    session,
    listenCode,
    listenMembers,
    listenEvents,
    startListen,
    joinListen,
    leaveListen,
    sendListenReaction,
    sendListenChat,
  } = usePlayer();
  const [joinCode, setJoinCode] = useState("");
  const [chat, setChat] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const createRoom = async () => {
    setBusy(true);
    setError("");
    try {
      const code = await startListen();
      setJoinCode(code);
      navigator.clipboard?.writeText(code).catch(() => undefined);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("createRoomFailed"));
    } finally {
      setBusy(false);
    }
  };

  const joinRoom = async () => {
    setBusy(true);
    setError("");
    try {
      await joinListen(joinCode);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("joinFailed"));
    } finally {
      setBusy(false);
    }
  };

  const submitChat = () => {
    const text = chat.trim();
    if (!text) return;
    sendListenChat(text);
    setChat("");
  };

  if (!listenCode) {
    return (
      <div className="rounded-xl border border-white/10 bg-white/[0.03] p-3">
        <div className="mb-2 flex items-center gap-2 text-sm font-bold text-neutral-200">
          <QueueIcon className="h-5 w-5 text-wave-pink" />
          <span>{t("listenTogether")}</span>
        </div>
        <div className="flex gap-2">
          <input
            value={joinCode}
            onChange={(event) => setJoinCode(event.target.value.toUpperCase())}
            placeholder={t("roomCode")}
            className="min-w-0 flex-1 rounded-lg border border-white/10 bg-black/20 px-3 py-2 text-sm font-bold uppercase outline-none transition focus:border-wave-pink/60"
          />
          <button
            type="button"
            onClick={joinRoom}
            disabled={busy || !joinCode.trim()}
            className="rounded-lg border border-wave-pink/25 px-3 py-2 text-sm font-bold text-wave-pink transition active:scale-95 disabled:opacity-50"
          >
            {t("join")}
          </button>
        </div>
        <button
          type="button"
          onClick={createRoom}
          disabled={busy}
          className="mt-2 flex w-full items-center justify-center gap-2 rounded-lg bg-wave-pink px-3 py-2 text-sm font-bold text-white transition active:scale-[0.98] disabled:opacity-60"
        >
          {busy ? <Spinner className="h-4 w-4 border-white/30 border-t-white" /> : null}
          <span>{t("createRoom")}</span>
        </button>
        {error ? <p className="mt-2 text-xs font-semibold text-red-300">{error}</p> : null}
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-wave-pink/20 bg-wave-pink/10 p-3">
      <div className="mb-2 flex items-center gap-2">
        <QueueIcon className="h-5 w-5 shrink-0 text-wave-pink" />
        <button
          type="button"
          onClick={() => navigator.clipboard?.writeText(listenCode).catch(() => undefined)}
          className="min-w-0 flex-1 truncate text-left text-sm font-black uppercase tracking-[0.12em] text-wave-pink"
        >
          {listenCode}
        </button>
        <button
          type="button"
          onClick={leaveListen}
          className="rounded-full border border-white/10 px-2 py-1 text-xs font-bold text-white/60 transition active:scale-95"
        >
          {t("leave")}
        </button>
      </div>
      <div className="mb-2 flex -space-x-2 overflow-hidden">
        {listenMembers.slice(0, 6).map((member) => (
          <Avatar
            key={member.username}
            username={member.username}
            name={member.displayName}
            className="h-7 w-7 border border-[#17050d]"
            textClass="text-xs"
          />
        ))}
      </div>
      <div className="mb-2 flex gap-1">
        {["🖤", "🔥", "✨", "😭"].map((emoji) => (
          <button
            key={emoji}
            type="button"
            onClick={() => sendListenReaction(emoji)}
            className="grid h-8 w-8 place-items-center rounded-full bg-white/5 text-sm transition active:scale-90"
          >
            {emoji}
          </button>
        ))}
      </div>
      <div className="flex gap-2">
        <input
          value={chat}
          onChange={(event) => setChat(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") submitChat();
          }}
          maxLength={280}
          placeholder={t("message")}
          className="min-w-0 flex-1 rounded-lg border border-white/10 bg-black/20 px-3 py-2 text-sm outline-none transition focus:border-wave-pink/60"
        />
        <button
          type="button"
          onClick={submitChat}
          disabled={!chat.trim()}
          className="rounded-lg bg-white/10 px-3 py-2 text-sm font-bold text-white/75 transition active:scale-95 disabled:opacity-40"
        >
          OK
        </button>
      </div>
      {listenEvents.length > 0 && (
        <div className="mt-2 max-h-24 space-y-1 overflow-auto pr-1 text-xs">
          {listenEvents.slice(-4).map((event) => (
            <div key={event.id} className="flex gap-1.5 text-white/55">
              <span className="shrink-0 font-bold text-white/70">
                {event.username === session.username ? t("you") : event.username}
              </span>
              <span className="min-w-0 flex-1 truncate">{event.text}</span>
              <span className="shrink-0 text-white/30">
                {timeAgo(Math.floor(event.atMs / 1000), t)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** Right-side Friend Activity feed: what everyone else is listening to. Tap a
 *  row to play that track. (For now every account on the instance is a friend.) */
export function FriendsPanel() {
  const { t } = useI18n();
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
          {t("friendsActivity")}
        </h2>
        {friends.length === 0 ? (
          <p className="rounded-xl border border-white/10 bg-white/[0.035] p-4 text-sm font-semibold text-neutral-500">
            {t("friendsQuiet")}
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
                  <span className="absolute -bottom-1 -right-1 rounded-full ring-2 ring-[#0d070b]">
                    <Avatar
                      username={friend.username}
                      name={friend.displayName}
                      className="h-5 w-5"
                      textClass="text-[10px]"
                    />
                  </span>
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-bold text-[#f3ecdd]">
                    {friend.displayName || friend.username}
                  </span>
                  <span className="block truncate text-xs text-neutral-400">
                    {friend.song.title} · {friend.song.artist}
                  </span>
                  <span className="block truncate text-[11px] text-neutral-500">
                    {timeAgo(friend.updatedAt, t)}
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
