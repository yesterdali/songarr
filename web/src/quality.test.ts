import { afterEach, describe, expect, it } from "vitest";

import { getDownloadQuality, qualityParams } from "./quality";

const originalNavigator = globalThis.navigator;

function mockConnection(connection: { saveData?: boolean; effectiveType?: string }) {
  Object.defineProperty(globalThis, "navigator", {
    value: { connection },
    configurable: true,
  });
}

afterEach(() => {
  Object.defineProperty(globalThis, "navigator", {
    value: originalNavigator,
    configurable: true,
  });
});

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

  it("auto follows data saver and slow network hints", () => {
    mockConnection({ saveData: true, effectiveType: "4g" });
    expect(qualityParams("auto")).toEqual({ format: "mp3", maxBitRate: 96 });

    mockConnection({ effectiveType: "3g" });
    expect(qualityParams("auto")).toEqual({ format: "mp3", maxBitRate: 192 });
  });

  it("download quality defaults to high without storage", () => {
    expect(getDownloadQuality()).toBe("high");
  });
});
