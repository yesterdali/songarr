import { describe, expect, it } from "vitest";

import { activeLyricsIndex, lyricsLoadState } from "./lyrics-ui";
import type { LyricsResult } from "./types";

function syncedLyrics(): LyricsResult {
  return {
    synced: true,
    lines: [
      { value: "one", start: 0 },
      { value: "two", start: 10_000 },
      { value: "three", start: 20_000 },
    ],
  };
}

describe("lyrics UI helpers", () => {
  it("finds the active synced line using playback time", () => {
    expect(activeLyricsIndex(syncedLyrics(), 0)).toBe(0);
    expect(activeLyricsIndex(syncedLyrics(), 10.1)).toBe(1);
    expect(activeLyricsIndex(syncedLyrics(), 23)).toBe(2);
  });

  it("does not mark an active line for plain lyrics", () => {
    expect(
      activeLyricsIndex(
        {
          synced: false,
          lines: [{ value: "plain" }],
        },
        12,
      ),
    ).toBe(-1);
  });

  it("maps loading, failure, empty, and ready states", () => {
    expect(lyricsLoadState(true, null, false)).toBe("loading");
    expect(lyricsLoadState(false, null, true)).toBe("error");
    expect(lyricsLoadState(false, null, false)).toBe("empty");
    expect(lyricsLoadState(false, { synced: false, lines: [] }, false)).toBe("idle");
    expect(lyricsLoadState(false, syncedLyrics(), false)).toBe("ready");
  });
});
