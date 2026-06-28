// Per-device streaming quality. Maps to the Subsonic `format` + `maxBitRate`
// the stream URL carries: Navidrome transcodes real tracks, and the proxy
// honors them for virtual (YouTube/Yandex/VK) tracks too.

export type StreamQuality = "auto" | "low" | "normal" | "high" | "lossless";

const KEY = "songarr.streamQuality";

export function getStreamQuality(): StreamQuality {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "auto" || v === "low" || v === "normal" || v === "high" || v === "lossless") {
      return v;
    }
  } catch {
    /* private mode */
  }
  return "high";
}

export function setStreamQuality(quality: StreamQuality): void {
  try {
    localStorage.setItem(KEY, quality);
  } catch {
    /* ignore */
  }
}

export type QualityParams = { format: string; maxBitRate: number };

export function qualityParams(quality: StreamQuality): QualityParams {
  const tier = quality === "auto" ? autoTier() : quality;
  switch (tier) {
    case "low":
      return { format: "mp3", maxBitRate: 96 };
    case "normal":
      return { format: "mp3", maxBitRate: 192 };
    case "lossless":
      return { format: "raw", maxBitRate: 0 };
    case "high":
    default:
      return { format: "mp3", maxBitRate: 320 };
  }
}

/** Resolve "auto" from the Network Information API (cellular / Data Saver → lower). */
function autoTier(): StreamQuality {
  try {
    const conn = (navigator as unknown as {
      connection?: { saveData?: boolean; effectiveType?: string };
    }).connection;
    if (conn?.saveData) return "low";
    switch (conn?.effectiveType) {
      case "slow-2g":
      case "2g":
        return "low";
      case "3g":
        return "normal";
      default:
        return "high"; // 4g / unknown
    }
  } catch {
    return "high";
  }
}
