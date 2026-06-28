import { translate, type MessageKey } from "./i18n";
import type { Song } from "./types";

function seed(reason: Song["reason"]): string {
  if (!reason?.seedArtist && !reason?.seedTitle) return "";
  if (reason.seedArtist && reason.seedTitle) return `${reason.seedArtist} — ${reason.seedTitle}`;
  return reason.seedTitle ?? reason.seedArtist ?? "";
}

type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string;

function defaultT(key: MessageKey, vars?: Record<string, string | number>): string {
  return translate("ru", key, vars);
}

export function reasonLabel(song: Song, t: Translate = defaultT): string | null {
  const reason = song.reason;
  if (!reason) return null;
  const seedText = seed(reason);
  switch (reason.kind) {
    case "because_liked":
      return seedText
        ? t("reasonBecauseLiked", { seed: seedText })
        : t("reasonBecauseLikedGeneric");
    case "because_played":
      return seedText
        ? t("reasonBecausePlayed", { seed: seedText })
        : t("reasonBecausePlayedGeneric");
    case "similar_to_current":
      return seedText ? t("reasonSimilar", { seed: seedText }) : t("reasonSimilarGeneric");
    case "yandex_wave":
      return t("reasonYandexWave");
    case "yandex_cache":
      return t("reasonYandexCache");
    case "library_random":
      return t("reasonLibraryRandom");
    default:
      return null;
  }
}
