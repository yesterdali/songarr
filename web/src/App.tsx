import { FormEvent, useEffect, useState } from "react";
import {
  clearSession,
  createSession,
  loadLastServerUrl,
  loadSession,
  normalizeServerUrl,
  saveSession,
  validateSession,
  type WaveSession,
} from "./auth";
import { getWaveNext } from "./api";
import {
  AccountButton,
  Avatar,
  DiscordConnectToggle,
  FriendsPanel,
  ListenTogetherPanel,
  NowPlayingBar,
  NowPlayingScreen,
  PlayBar,
} from "./components";
import { DownloadIcon, LibraryIcon, PlaylistIcon, SearchIcon, WaveIcon } from "./icons";
import { DownloadsProvider } from "./downloads";
import { NavProvider, useNav, type Route, type TabName } from "./nav";
import { PlayerProvider } from "./player";
import {
  getDownloadQuality,
  getStreamQuality,
  setDownloadQuality,
  setStreamQuality,
  type DownloadQuality,
  type StreamQuality,
} from "./quality";
import {
  AlbumsView,
  AlbumView,
  ArtistLookupView,
  ArtistView,
  HomeView,
  ImportsView,
  LibraryView,
  LikedView,
  PlaylistsView,
  PlaylistView,
  SearchView,
  SettingsView,
} from "./views";

const STREAM_QUALITY_CHOICES: [StreamQuality, string][] = [
  ["auto", "Авто"],
  ["low", "96"],
  ["normal", "192"],
  ["high", "320"],
  ["lossless", "Оригинал"],
];

const DOWNLOAD_QUALITY_CHOICES: [DownloadQuality, string][] = [
  ["low", "96"],
  ["normal", "192"],
  ["high", "320"],
  ["lossless", "Оригинал"],
];

function SetupStep({
  index,
  active,
  children,
}: {
  index: number;
  active: boolean;
  children: string;
}) {
  return (
    <span
      className={`inline-flex items-center gap-2 rounded-full px-3 py-1 text-xs font-bold ${
        active
          ? "bg-wave-pink/15 text-wave-pink ring-1 ring-wave-pink/25"
          : "bg-black/[0.04] text-neutral-500 dark:bg-white/[0.04]"
      }`}
    >
      <span className="grid h-5 w-5 place-items-center rounded-full bg-current/10 text-[11px]">
        {index}
      </span>
      {children}
    </span>
  );
}

function QualityButtons<T extends string>({
  value,
  choices,
  onChange,
}: {
  value: T;
  choices: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
      {choices.map(([choice, label]) => (
        <button
          key={choice}
          type="button"
          onClick={() => onChange(choice)}
          className={`rounded-xl border px-3 py-2 text-sm font-bold transition active:scale-95 ${
            value === choice
              ? "border-wave-pink/40 bg-wave-pink/10 text-wave-pink"
              : "border-black/10 text-neutral-600 hover:bg-black/[0.04] dark:border-white/10 dark:text-neutral-300 dark:hover:bg-white/[0.04]"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function LoginScreen({ onLogin }: { onLogin: (session: WaveSession) => void }) {
  const [serverUrl, setServerUrl] = useState(() => loadLastServerUrl());
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [draftSession, setDraftSession] = useState<WaveSession | null>(null);
  const [streamQuality, setStreamQualityState] = useState<StreamQuality>(getStreamQuality);
  const [downloadQuality, setDownloadQualityState] =
    useState<DownloadQuality>(getDownloadQuality);
  const [waveStatus, setWaveStatus] = useState<"idle" | "ok" | "warn">("idle");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  function nextServer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    const normalized = normalizeServerUrl(serverUrl);
    try {
      new URL(normalized);
    } catch {
      setError("Проверь адрес Songarr");
      return;
    }
    setServerUrl(normalized);
    setStep(2);
  }

  async function submitLogin(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setWaveStatus("idle");
    setBusy(true);
    try {
      const session = createSession(username, password, serverUrl);
      await validateSession(session);
      setDraftSession(session);
      setPassword("");
      setStep(3);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Не удалось войти");
    } finally {
      setBusy(false);
    }
  }

  async function testWave() {
    const session = draftSession;
    if (!session) return;
    setError("");
    setBusy(true);
    try {
      const songs = await getWaveNext(session, { count: 1 });
      if (songs.length === 0) {
        setWaveStatus("warn");
        setError("Волна пока не вернула треки. Войти можно, но рекомендации могут появиться позже.");
      } else {
        setWaveStatus("ok");
      }
    } catch (err) {
      setWaveStatus("warn");
      setError(err instanceof Error ? err.message : "Не удалось проверить волну");
    } finally {
      setBusy(false);
    }
  }

  function finish() {
    const session = draftSession;
    if (!session) return;
    setStreamQuality(streamQuality);
    setDownloadQuality(downloadQuality);
    saveSession(session);
    onLogin(session);
  }

  return (
    <div className="min-h-full">
      <main className="mx-auto flex min-h-full max-w-md animate-fade-in flex-col justify-center px-6 py-10">
        <div className="mb-9 flex flex-col items-center text-center">
          <div className="mb-4 grid h-16 w-16 place-items-center rounded-lg bg-gradient-to-br from-wave-orange via-wave-pink to-wave-violet shadow-lg shadow-wave-pink/30">
            <WaveIcon className="h-8 w-8 text-white" />
          </div>
          <h1 className="text-4xl font-extrabold tracking-tight">Твоя волна</h1>
          <p className="font-display mt-1 text-base italic tracking-[0.2em] text-wave-pink/80">
            Songarr Wave
          </p>
        </div>
        <div className="mb-4 flex flex-wrap justify-center gap-2">
          <SetupStep index={1} active={step === 1}>Сервер</SetupStep>
          <SetupStep index={2} active={step === 2}>Вход</SetupStep>
          <SetupStep index={3} active={step === 3}>Звук</SetupStep>
        </div>

        {step === 1 && (
          <form
            className="space-y-4 rounded-xl border border-black/5 bg-white/70 p-6 shadow-xl shadow-black/5 backdrop-blur-xl dark:border-white/10 dark:bg-white/[0.04]"
            onSubmit={nextServer}
          >
            <label className="block">
              <span className="mb-1.5 block text-sm font-semibold">Songarr URL</span>
              <input
                className="w-full rounded-xl border border-black/10 bg-white px-4 py-3 text-base outline-none transition focus:border-wave-pink focus:ring-2 focus:ring-wave-pink/25 dark:border-white/10 dark:bg-white/5"
                autoComplete="url"
                inputMode="url"
                placeholder="https://songarr.example.com"
                value={serverUrl}
                onChange={(event) => setServerUrl(event.target.value)}
                required
              />
            </label>
            {error ? (
              <p className="rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm font-medium text-red-600 dark:text-red-300">
                {error}
              </p>
            ) : null}
            <button
              type="submit"
              className="w-full rounded-xl bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-3 text-base font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-[0.98]"
            >
              Далее
            </button>
          </form>
        )}

        {step === 2 && (
          <form
          className="space-y-4 rounded-xl border border-black/5 bg-white/70 p-6 shadow-xl shadow-black/5 backdrop-blur-xl dark:border-white/10 dark:bg-white/[0.04]"
            onSubmit={submitLogin}
          >
            <p className="truncate rounded-xl bg-black/[0.04] px-4 py-3 text-sm font-semibold text-neutral-500 dark:bg-white/[0.04]">
              {serverUrl}
            </p>
            <label className="block">
              <span className="mb-1.5 block text-sm font-semibold">Username</span>
              <input
                className="w-full rounded-xl border border-black/10 bg-white px-4 py-3 text-base outline-none transition focus:border-wave-pink focus:ring-2 focus:ring-wave-pink/25 dark:border-white/10 dark:bg-white/5"
                autoComplete="username"
                value={username}
                onChange={(event) => setUsername(event.target.value)}
                required
              />
            </label>
            <label className="block">
              <span className="mb-1.5 block text-sm font-semibold">Password</span>
              <input
                className="w-full rounded-xl border border-black/10 bg-white px-4 py-3 text-base outline-none transition focus:border-wave-pink focus:ring-2 focus:ring-wave-pink/25 dark:border-white/10 dark:bg-white/5"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                required
              />
            </label>
            {error ? (
              <p className="rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm font-medium text-red-600 dark:text-red-300">
                {error}
              </p>
            ) : null}
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => setStep(1)}
                className="rounded-xl border border-black/10 px-4 py-3 text-sm font-bold text-neutral-600 transition active:scale-[0.98] dark:border-white/10 dark:text-neutral-300"
              >
                Назад
              </button>
              <button
                type="submit"
                disabled={busy}
                className="flex-1 rounded-xl bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-3 text-base font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-[0.98] disabled:opacity-60"
              >
                {busy ? "Проверяю..." : "Войти"}
              </button>
            </div>
          </form>
        )}

        {step === 3 && (
          <div className="space-y-4 rounded-xl border border-black/5 bg-white/70 p-6 shadow-xl shadow-black/5 backdrop-blur-xl dark:border-white/10 dark:bg-white/[0.04]">
            <div>
              <p className="mb-2 text-xs font-bold uppercase tracking-[0.16em] text-neutral-500">
                Качество стрима
              </p>
              <QualityButtons
                value={streamQuality}
                choices={STREAM_QUALITY_CHOICES}
                onChange={setStreamQualityState}
              />
            </div>
            <div>
              <p className="mb-2 text-xs font-bold uppercase tracking-[0.16em] text-neutral-500">
                Качество загрузок
              </p>
              <QualityButtons
                value={downloadQuality}
                choices={DOWNLOAD_QUALITY_CHOICES}
                onChange={setDownloadQualityState}
              />
            </div>
            {waveStatus === "ok" ? (
              <p className="rounded-xl border border-emerald-500/20 bg-emerald-500/10 px-4 py-3 text-sm font-medium text-emerald-600 dark:text-emerald-300">
                Волна отвечает. Можно слушать.
              </p>
            ) : null}
            {error ? (
              <p className="rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm font-medium text-red-600 dark:text-red-300">
                {error}
              </p>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={testWave}
                disabled={busy}
                className="rounded-xl border border-black/10 px-4 py-3 text-sm font-bold text-neutral-600 transition active:scale-[0.98] disabled:opacity-60 dark:border-white/10 dark:text-neutral-300"
              >
                {busy ? "Проверяю..." : "Проверить волну"}
              </button>
              <button
                type="button"
                onClick={finish}
                className="flex-1 rounded-xl bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-3 text-base font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-[0.98]"
              >
                Начать
              </button>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

function CurrentScreen({ route, onLogout }: { route: Route; onLogout: () => void }) {
  switch (route.name) {
    case "home":
      return <HomeView />;
    case "search":
      return <SearchView />;
    case "library":
      return <LibraryView />;
    case "albums":
      return <AlbumsView />;
    case "playlists":
      return <PlaylistsView />;
    case "liked":
      return <LikedView />;
    case "imports":
      return <ImportsView />;
    case "artist":
      return <ArtistView id={route.id} title={route.title} />;
    case "artistLookup":
      return <ArtistLookupView title={route.title} />;
    case "album":
      return <AlbumView id={route.id} title={route.title} />;
    case "playlist":
      return <PlaylistView id={route.id} title={route.title} />;
    case "settings":
      return <SettingsView onLogout={onLogout} />;
  }
}

const TABS: { tab: TabName; label: string; icon: typeof WaveIcon }[] = [
  { tab: "home", label: "Волна", icon: WaveIcon },
  { tab: "search", label: "Поиск", icon: SearchIcon },
  { tab: "library", label: "Медиатека", icon: LibraryIcon },
  { tab: "playlists", label: "Плейлисты", icon: PlaylistIcon },
];

function DesktopSidebar({ activeTab }: { activeTab: TabName }) {
  const nav = useNav();
  return (
    <aside className="hidden min-h-dvh border-r border-wave-pink/10 px-5 py-6 lg:flex lg:flex-col">
      <button
        type="button"
        onClick={() => nav.setTab("home")}
        className="mb-8 flex items-center gap-3 text-left"
      >
        <span className="grid h-11 w-11 place-items-center rounded-lg bg-gradient-to-br from-wave-orange via-wave-pink to-wave-violet shadow-lg shadow-wave-pink/20">
          <WaveIcon className="h-6 w-6 text-white" />
        </span>
        <span>
          <span className="block text-lg font-extrabold leading-none">Songarr</span>
          <span className="font-display text-sm italic tracking-[0.18em] text-wave-pink/75">
            Wave
          </span>
        </span>
      </button>

      <div className="mb-6 space-y-3">
        <AccountButton />
        <DiscordConnectToggle />
        <ListenTogetherPanel />
      </div>

      <nav className="space-y-1">
        {TABS.map(({ tab, label, icon: Icon }) => {
          const active = activeTab === tab;
          return (
            <button
              key={tab}
              type="button"
              onClick={() => nav.setTab(tab)}
              className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-bold transition ${
                active
                  ? "bg-wave-pink/10 text-[#f3ecdd] ring-1 ring-wave-pink/20"
                  : "text-neutral-500 hover:bg-white/[0.04] hover:text-neutral-200"
              }`}
            >
              <Icon className={`h-5 w-5 ${active ? "text-wave-pink" : ""}`} />
              {label}
            </button>
          );
        })}
      </nav>
      <button
        type="button"
        onClick={() => nav.push({ name: "imports" })}
        className="mt-6 flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-bold text-neutral-500 transition hover:bg-white/[0.04] hover:text-neutral-200"
      >
        <DownloadIcon className="h-5 w-5" />
        Импорт
      </button>

    </aside>
  );
}

function Shell({ session, onLogout }: { session: WaveSession; onLogout: () => void }) {
  const [stack, setStack] = useState<Route[]>([{ name: "home" }]);
  const [npOpen, setNpOpen] = useState(false);
  const route = stack[stack.length - 1];
  const activeTab = stack[0].name as TabName;

  const nav = {
    route,
    push: (next: Route) => setStack((s) => [...s, next]),
    back: () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s)),
    setTab: (tab: TabName) => setStack([{ name: tab }]),
    canGoBack: stack.length > 1,
  };

  return (
    <DownloadsProvider session={session}>
      <PlayerProvider session={session}>
      <NavProvider value={nav}>
        <div className="min-h-dvh">
          <div className="mx-auto grid min-h-dvh w-full max-w-[1500px] lg:grid-cols-[240px_minmax(0,1fr)] xl:grid-cols-[240px_minmax(0,1fr)_340px]">
            <DesktopSidebar activeTab={activeTab} />

            <main className="min-w-0 px-5 pb-44 pt-6 md:px-8 lg:pb-28 lg:pt-8 xl:px-10">
              <div className="mx-auto w-full max-w-md md:max-w-3xl lg:max-w-5xl">
                {route.name === "home" && (
                  <header className="mb-6 flex items-center gap-3 lg:hidden">
                    <h1 className="flex-1 text-[28px] font-extrabold tracking-tight">
                      Музыка
                    </h1>
                    <button
                      type="button"
                      onClick={() => nav.push({ name: "settings" })}
                      aria-label="settings"
                      className="rounded-full transition active:scale-95"
                    >
                      <Avatar username={session.username} className="h-9 w-9" />
                    </button>
                  </header>
                )}
                {route.name === "home" && (
                  <div className="mb-5 lg:hidden">
                    <ListenTogetherPanel />
                  </div>
                )}
                <CurrentScreen route={route} onLogout={onLogout} />
              </div>
            </main>

            <FriendsPanel />
          </div>

          {/* Desktop: persistent bottom playbar (Spotify/Flutter style). */}
          <div className="fixed inset-x-0 bottom-0 z-20">
            <PlayBar onOpen={() => setNpOpen(true)} />
          </div>

          {/* Bottom chrome: floating dock with the now-playing bar above the tab bar */}
          <div className="fixed inset-x-0 bottom-0 z-10 px-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] lg:hidden">
            <div className="mx-auto max-w-md overflow-hidden rounded-xl border border-black/5 bg-white/85 shadow-2xl shadow-black/15 backdrop-blur-2xl dark:border-wave-pink/15 dark:bg-[#0d070b]/90">
              <NowPlayingBar onOpen={() => setNpOpen(true)} />
              <nav className="flex items-stretch justify-around">
                {TABS.map(({ tab, label, icon: Icon }) => {
                  const active = activeTab === tab;
                  return (
                    <button
                      key={tab}
                      type="button"
                      onClick={() => nav.setTab(tab)}
                      className={`flex flex-1 flex-col items-center gap-0.5 pb-2.5 pt-2 text-[11px] font-semibold transition-colors ${
                        active
                          ? "text-wave-pink"
                          : "text-neutral-500 dark:text-neutral-400"
                      }`}
                    >
                      <Icon
                        className={`h-6 w-6 transition-transform duration-200 ${
                          active ? "-translate-y-0.5 scale-110" : ""
                        }`}
                      />
                      {label}
                    </button>
                  );
                })}
              </nav>
            </div>
          </div>

          {npOpen && <NowPlayingScreen onClose={() => setNpOpen(false)} />}
        </div>
      </NavProvider>
      </PlayerProvider>
    </DownloadsProvider>
  );
}

export default function App() {
  const [session, setSession] = useState<WaveSession | null>(() => loadSession());

  useEffect(() => {
    if (!session) return;
    validateSession(session).catch(() => {
      clearSession();
      setSession(null);
    });
  }, [session]);

  if (!session) {
    return <LoginScreen onLogin={setSession} />;
  }
  return (
    <Shell
      session={session}
      onLogout={() => {
        clearSession();
        setSession(null);
      }}
    />
  );
}
