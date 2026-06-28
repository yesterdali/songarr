import type { LyricsResult } from "./types";

export type LyricsLoadState = "idle" | "loading" | "ready" | "empty" | "error";

export function activeLyricsIndex(lyrics: LyricsResult | null, currentTime: number): number {
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

export function lyricsLoadState(
  loading: boolean,
  lyrics: LyricsResult | null,
  failed: boolean,
): LyricsLoadState {
  if (loading) return "loading";
  if (failed) return "error";
  if (lyrics && lyrics.lines.length > 0) return "ready";
  if (lyrics === null) return "empty";
  return "idle";
}
