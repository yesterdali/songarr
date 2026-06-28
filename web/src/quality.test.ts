import { describe, expect, it } from "vitest";

import { qualityParams } from "./quality";

describe("qualityParams", () => {
  it("maps each tier to a format + bitrate", () => {
    expect(qualityParams("low")).toEqual({ format: "mp3", maxBitRate: 96 });
    expect(qualityParams("normal")).toEqual({ format: "mp3", maxBitRate: 192 });
    expect(qualityParams("high")).toEqual({ format: "mp3", maxBitRate: 320 });
    expect(qualityParams("lossless")).toEqual({ format: "raw", maxBitRate: 0 });
  });

  it("auto falls back to high when no network info is available", () => {
    expect(qualityParams("auto")).toEqual({ format: "mp3", maxBitRate: 320 });
  });
});
