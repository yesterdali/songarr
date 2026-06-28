import { getServerTime, streamUrl } from "./api";
import type { WaveSession } from "./auth";
import type { RemoteState } from "./types";
import type { Song } from "./types";

const VOLUME_KEY = "songarr.wave.volume.v1";

/** A track reduced to the fields the bot needs in a remote `play` command. */
export function toRemoteTrack(song: Song) {
  return {
    id: song.id,
    title: song.title,
    artist: song.artist,
    album: song.album ?? null,
    coverArt: song.coverArt ?? null,
    duration: song.duration ?? null,
    durationMs: song.duration ? Math.round(song.duration * 1000) : null,
    provider: song.provider ?? null,
    artistId: song.artistId ?? null,
    albumId: song.albumId ?? null,
  };
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function audioDuration(audio: HTMLAudioElement, fallback?: number): number {
  if (Number.isFinite(audio.duration) && audio.duration > 0) return audio.duration;
  return fallback && Number.isFinite(fallback) && fallback > 0 ? fallback : 0;
}

export function loadVolume(): number {
  const raw = localStorage.getItem(VOLUME_KEY);
  const parsed = raw ? Number(raw) : 0.85;
  return Number.isFinite(parsed) ? clamp(parsed, 0, 1) : 0.85;
}

export function saveVolume(volume: number): void {
  localStorage.setItem(VOLUME_KEY, String(volume));
}

export function streamSongUrl(session: WaveSession, song: Song): string {
  return song.streamUrl ?? streamUrl(session, song.id);
}

export async function estimateServerClockOffset(session: WaveSession): Promise<number> {
  let bestOffset = 0;
  let bestRtt = Number.POSITIVE_INFINITY;
  for (let i = 0; i < 4; i += 1) {
    const started = Date.now();
    const serverMs = await getServerTime(session);
    const ended = Date.now();
    const rtt = ended - started;
    if (rtt < bestRtt) {
      bestRtt = rtt;
      bestOffset = serverMs + rtt / 2 - ended;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 30));
  }
  return bestOffset;
}

export function remotePlaybackSeconds(remoteOn: boolean, state: RemoteState | null): number {
  if (!remoteOn || !state) return 0;
  const base = (state.positionMs ?? 0) / 1000;
  if (!state.isPlaying) return base;
  const duration = (state.durationMs ?? 0) / 1000;
  const current = base + (Date.now() - state.fetchedAt) / 1000;
  return duration > 0 ? Math.min(current, duration) : current;
}

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const total = Math.floor(seconds);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
