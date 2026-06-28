// Per-device streaming quality. Maps to the Subsonic `format` + `maxBitRate`
// the stream URL carries: Navidrome transcodes real tracks, and the proxy
// honors them for virtual (YouTube/Yandex/VK) tracks too.

export type DownloadQuality = "low" | "normal" | "high" | "lossless";
export type StreamQuality = "auto" | DownloadQuality;

const STREAM_KEY = "songarr.streamQuality";
const DOWNLOAD_KEY = "songarr.downloadQuality";

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

export function qualityLabel(quality: StreamQuality | DownloadQuality): string {
  const params = qualityParams(quality);
  if (isLosslessParams(params)) return "Оригинал";
  return `MP3 ${params.maxBitRate} кбит/с`;
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
