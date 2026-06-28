import type { Song } from "./types";

function seed(reason: Song["reason"]): string {
  if (!reason?.seedArtist && !reason?.seedTitle) return "";
  if (reason.seedArtist && reason.seedTitle) return `${reason.seedArtist} — ${reason.seedTitle}`;
  return reason.seedTitle ?? reason.seedArtist ?? "";
}

export function reasonLabel(song: Song): string | null {
  const reason = song.reason;
  if (!reason) return null;
  const seedText = seed(reason);
  switch (reason.kind) {
    case "because_liked":
      return seedText ? `Потому что понравилось ${seedText}` : "Потому что тебе это нравится";
    case "because_played":
      return seedText ? `После ${seedText}` : "По недавним прослушиваниям";
    case "similar_to_current":
      return seedText ? `Похоже на ${seedText}` : "Похожий трек";
    case "yandex_wave":
      return "Из Яндекс Волны";
    case "yandex_cache":
      return "Из Яндекс рекомендаций";
    case "library_random":
      return "Из твоей библиотеки";
    default:
      return null;
  }
}
