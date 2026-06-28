import { useEffect, useState, type ChangeEvent } from "react";
import * as api from "./api";
import { Avatar, Segmented } from "./components";
import {
  DOWNLOAD_QUALITY_CHOICES,
  getDownloadQuality,
  getStreamQuality,
  setDownloadQuality,
  setStreamQuality,
  STREAM_QUALITY_CHOICES,
  type DownloadQuality,
  type StreamQuality,
} from "./quality";
import { LANGUAGES, useI18n } from "./i18n";
import { usePlayer } from "./player";
import { ScreenHeader, SectionTitle } from "./views";

/** Center-crop + downscale an image file to a square JPEG for the avatar. */
function resizeImage(file: File, size: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = size;
      canvas.height = size;
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        reject(new Error("canvas unavailable"));
        return;
      }
      const min = Math.min(img.width, img.height);
      const sx = (img.width - min) / 2;
      const sy = (img.height - min) / 2;
      ctx.drawImage(img, sx, sy, min, min, 0, 0, size, size);
      URL.revokeObjectURL(img.src);
      canvas.toBlob(
        (blob) => (blob ? resolve(blob) : reject(new Error("encode failed"))),
        "image/jpeg",
        0.85,
      );
    };
    img.onerror = () => reject(new Error("invalid image"));
    img.src = URL.createObjectURL(file);
  });
}

export function SettingsView({ onLogout }: { onLogout: () => void }) {
  const { session } = usePlayer();
  const { language, setLanguage, t } = useI18n();
  const [name, setName] = useState("");
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [avatarVer, setAvatarVer] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [streamQuality, setStreamQualityState] = useState<StreamQuality>(getStreamQuality);
  const [downloadQuality, setDownloadQualityState] =
    useState<DownloadQuality>(getDownloadQuality);

  useEffect(() => {
    api
      .getProfile(session)
      .then((p) => setName(p.displayName ?? ""))
      .catch(() => undefined);
  }, [session]);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.setDisplayName(session, name.trim());
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch {
      setError(t("saveFailed"));
    } finally {
      setBusy(false);
    }
  }

  async function onFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      const blob = await resizeImage(file, 256);
      await api.uploadAvatar(session, blob);
      setAvatarVer((v) => v + 1);
    } catch {
      setError(t("avatarFailed"));
    } finally {
      setBusy(false);
    }
  }

  async function clearAvatar() {
    setBusy(true);
    try {
      await api.removeAvatar(session);
      setAvatarVer((v) => v + 1);
    } catch {
      // ignore
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="animate-fade-in max-w-md">
      <ScreenHeader title={t("settings")} />
      <section className="mb-8">
        <SectionTitle>{t("profile")}</SectionTitle>
        <div className="mb-5 flex items-center gap-4">
          <Avatar
            username={session.username}
            name={name || session.username}
            className="h-16 w-16"
            textClass="text-xl"
            bust={avatarVer}
          />
          <div className="space-y-2">
            <label className="inline-block cursor-pointer rounded-full bg-wave-pink px-4 py-2 text-sm font-bold text-white transition active:scale-95">
              {t("uploadAvatar")}
              <input type="file" accept="image/*" className="hidden" onChange={onFile} />
            </label>
            <button
              type="button"
              onClick={clearAvatar}
              className="block text-xs font-semibold text-neutral-500 transition hover:text-neutral-300"
            >
              {t("removeAvatar")}
            </button>
          </div>
        </div>
        <label className="block">
          <span className="mb-1.5 block text-sm font-semibold">{t("displayName")}</span>
          <input
            value={name}
            maxLength={40}
            placeholder={session.username}
            onChange={(event) => setName(event.target.value)}
            className="w-full rounded-xl border border-black/10 bg-white px-4 py-3 text-base outline-none transition focus:border-wave-pink focus:ring-2 focus:ring-wave-pink/25 dark:border-white/10 dark:bg-white/5"
          />
        </label>
        {error && <p className="mt-2 text-sm font-medium text-red-500">{error}</p>}
        <button
          type="button"
          onClick={save}
          disabled={busy}
          className="mt-3 rounded-full bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-2.5 font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-95 disabled:opacity-60"
        >
          {saved ? t("saved") : t("save")}
        </button>
      </section>
      <section className="mb-8">
        <SectionTitle>{t("language")}</SectionTitle>
        <Segmented
          value={language}
          onChange={setLanguage}
          className="grid grid-cols-3 gap-2"
          options={LANGUAGES.map((option) => ({ value: option.code, label: option.name }))}
        />
      </section>
      <section className="mb-8">
        <SectionTitle>{t("audioQuality")}</SectionTitle>
        <p className="mb-2 text-xs font-semibold uppercase tracking-[0.16em] text-neutral-500">
          {t("stream")}
        </p>
        <Segmented
          value={streamQuality}
          onChange={(value) => {
            setStreamQualityState(value);
            setStreamQuality(value);
          }}
          className="grid grid-cols-2 gap-2 sm:grid-cols-5"
          options={STREAM_QUALITY_CHOICES.map(([value, key]) => ({ value, label: t(key) }))}
        />
        <p className="mt-2 text-xs text-neutral-500 dark:text-neutral-400">
          {t("streamHint")}
        </p>
        <p className="mb-2 mt-5 text-xs font-semibold uppercase tracking-[0.16em] text-neutral-500">
          {t("downloads")}
        </p>
        <Segmented
          value={downloadQuality}
          onChange={(value) => {
            setDownloadQualityState(value);
            setDownloadQuality(value);
          }}
          className="grid grid-cols-2 gap-2 sm:grid-cols-4"
          options={DOWNLOAD_QUALITY_CHOICES.map(([value, key]) => ({ value, label: t(key) }))}
        />
        <p className="mt-2 text-xs text-neutral-500 dark:text-neutral-400">
          {t("downloadHint")}
        </p>
      </section>
      <section>
        <SectionTitle>{t("account")}</SectionTitle>
        <p className="mb-3 text-sm text-neutral-500 dark:text-neutral-400">
          {t("signedIn")}: <span className="font-semibold">{session.username}</span>
        </p>
        <button
          type="button"
          onClick={onLogout}
          className="rounded-lg border border-black/10 px-4 py-2 text-sm font-bold text-neutral-600 transition hover:bg-black/[0.04] dark:border-white/10 dark:text-neutral-300 dark:hover:bg-white/[0.04]"
        >
          {t("logout")}
        </button>
      </section>
    </div>
  );
}
