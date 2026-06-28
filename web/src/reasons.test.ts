import { describe, expect, it } from "vitest";

import { reasonLabel } from "./reasons";
import type { Song } from "./types";

function song(reason: Song["reason"]): Song {
  return {
    id: "1",
    title: "Track",
    artist: "Artist",
    reason,
  };
}

describe("reasonLabel", () => {
  it("formats seeded similarity labels", () => {
    expect(
      reasonLabel(
        song({
          kind: "similar_to_current",
          seedArtist: "Янка",
          seedTitle: "Про чертиков",
        }),
      ),
    ).toBe("Похоже на Янка — Про чертиков");
  });

  it("formats source-only labels", () => {
    expect(reasonLabel(song({ kind: "yandex_wave", source: "yandex" }))).toBe(
      "Из Яндекс Волны",
    );
    expect(reasonLabel(song({ kind: "library_random", source: "library" }))).toBe(
      "Из твоей библиотеки",
    );
  });
});
