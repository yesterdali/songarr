import { describe, expect, it } from "vitest";

import { audioDuration } from "./player-utils";

const fakeAudio = (duration: number) => ({ duration }) as HTMLAudioElement;

describe("audioDuration", () => {
  it("prefers the stable metadata duration over the element's duration", () => {
    // A transcoded/streamed source reports a duration that grows as bytes
    // arrive; the known track length must win so the progress bar stays stable.
    expect(audioDuration(fakeAudio(3), 152)).toBe(152);
    expect(audioDuration(fakeAudio(140), 152)).toBe(152);
  });

  it("falls back to the element's duration when no metadata is known", () => {
    expect(audioDuration(fakeAudio(152), undefined)).toBe(152);
    expect(audioDuration(fakeAudio(152), 0)).toBe(152);
  });

  it("ignores a non-finite element duration", () => {
    expect(audioDuration(fakeAudio(Infinity), undefined)).toBe(0);
    expect(audioDuration(fakeAudio(NaN), 152)).toBe(152);
  });

  it("returns 0 when nothing is known", () => {
    expect(audioDuration(fakeAudio(0), undefined)).toBe(0);
  });
});
