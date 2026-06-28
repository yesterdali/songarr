import type { MessageKey } from "./i18n";

// Per-device streaming quality. Maps to the Subsonic `format` + `maxBitRate`
// the stream URL carries: Navidrome transcodes real tracks, and the proxy
// honors them for virtual (YouTube/Yandex/VK) tracks too.

export type DownloadQuality = "low" | "normal" | "high" | "lossless";
export type StreamQuality = "auto" | DownloadQuality;

const STREAM_KEY = "songarr.streamQuality";
const DOWNLOAD_KEY = "songarr.downloadQuality";

/** Canonical tier → i18n-label lists, shared by onboarding and settings. */
export const STREAM_QUALITY_CHOICES: [StreamQuality, MessageKey][] = [
  ["auto", "qualityAuto"],
  ["low", "qualityLow"],
  ["normal", "qualityNormal"],
  ["high", "qualityHigh"],
  ["lossless", "qualityOriginal"],
];

export const DOWNLOAD_QUALITY_CHOICES: [DownloadQuality, MessageKey][] = [
  ["low", "qualityLow"],
  ["normal", "qualityNormal"],
  ["high", "qualityHigh"],
  ["lossless", "qualityOriginal"],
];

function isStreamQuality(value: string | null): value is StreamQuality {
  return (
    value === "auto" ||
    value === "low" ||
    value === "normal" ||
    value === "high" ||
    value === "lossless"
  );
}

function isDownloadQuality(value: string | null): value is DownloadQuality {
  return value === "low" || value === "normal" || value === "high" || value === "lossless";
}

export function getStreamQuality(): StreamQuality {
  try {
    const v = localStorage.getItem(STREAM_KEY);
    if (isStreamQuality(v)) {
      return v;
    }
  } catch {
    /* private mode */
  }
  return "high";
}

export function setStreamQuality(quality: StreamQuality): void {
  try {
    localStorage.setItem(STREAM_KEY, quality);
  } catch {
    /* ignore */
  }
}

export function getDownloadQuality(): DownloadQuality {
  try {
    const v = localStorage.getItem(DOWNLOAD_KEY);
    if (isDownloadQuality(v)) return v;
  } catch {
    /* private mode */
  }
  return "high";
}

export function setDownloadQuality(quality: DownloadQuality): void {
  try {
    localStorage.setItem(DOWNLOAD_KEY, quality);
  } catch {
    /* ignore */
  }
}

export type QualityParams = { format: string; maxBitRate: number };

export function qualityParams(quality: StreamQuality | DownloadQuality): QualityParams {
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

export function fallbackQualityParams(): QualityParams {
  return { format: "mp3", maxBitRate: 320 };
}

export function isLosslessParams(params: QualityParams): boolean {
  return params.format === "raw" || params.maxBitRate === 0;
}

type Translator = (key: MessageKey, vars?: Record<string, string | number>) => string;

function defaultTranslate(key: MessageKey, vars: Record<string, string | number> = {}): string {
  if (key === "qualityOriginal") return "Original";
  if (key === "qualityMp3Kbps") return `MP3 ${vars.bitrate} kbps`;
  return key;
}

export function qualityLabel(
  quality: StreamQuality | DownloadQuality,
  t: Translator = defaultTranslate,
): string {
  const params = qualityParams(quality);
  if (isLosslessParams(params)) return t("qualityOriginal");
  return t("qualityMp3Kbps", { bitrate: params.maxBitRate });
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
