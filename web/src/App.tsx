import { FormEvent, useEffect, useState } from "react";
import {
  clearSession,
  createSession,
  loadLastServerUrl,
  loadSession,
  saveSession,
  validateSession,
  type WaveSession,
} from "./auth";
import { Cover, NowPlayingBar, NowPlayingScreen } from "./components";
import {
  LibraryIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PlaylistIcon,
  SearchIcon,
  WaveIcon,
} from "./icons";
import { NavProvider, useNav, type Route, type TabName } from "./nav";
import { formatTime, PlayerProvider, usePlayer } from "./player";
import {
  AlbumsView,
  AlbumView,
  ArtistLookupView,
  ArtistView,
  HomeView,
  LibraryView,
  LikedView,
  PlaylistsView,
  PlaylistView,
  SearchView,
} from "./views";

function LoginScreen({ onLogin }: { onLogin: (session: WaveSession) => void }) {
  const [serverUrl, setServerUrl] = useState(() => loadLastServerUrl());
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setBusy(true);
    try {
      const session = createSession(username, password, serverUrl);
      await validateSession(session);
      saveSession(session);
      setPassword("");
      onLogin(session);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed");
    } finally {
      setBusy(false);
    }
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
        <form
          className="space-y-4 rounded-xl border border-black/5 bg-white/70 p-6 shadow-xl shadow-black/5 backdrop-blur-xl dark:border-white/10 dark:bg-white/[0.04]"
          onSubmit={submit}
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
          <button
            type="submit"
            disabled={busy}
            className="w-full rounded-xl bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-3 text-base font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-[0.98] disabled:opacity-60"
          >
            {busy ? "Checking..." : "Log in"}
          </button>
        </form>
      </main>
    </div>
  );
}

function CurrentScreen({ route }: { route: Route }) {
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
    case "artist":
      return <ArtistView id={route.id} title={route.title} />;
    case "artistLookup":
      return <ArtistLookupView title={route.title} />;
    case "album":
      return <AlbumView id={route.id} title={route.title} />;
    case "playlist":
      return <PlaylistView id={route.id} title={route.title} />;
  }
}

const TABS: { tab: TabName; label: string; icon: typeof WaveIcon }[] = [
  { tab: "home", label: "Волна", icon: WaveIcon },
  { tab: "search", label: "Поиск", icon: SearchIcon },
  { tab: "library", label: "Медиатека", icon: LibraryIcon },
  { tab: "playlists", label: "Плейлисты", icon: PlaylistIcon },
];

function DesktopSidebar({
  activeTab,
  session,
  onLogout,
}: {
  activeTab: TabName;
  session: WaveSession;
  onLogout: () => void;
}) {
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

      <div className="mt-auto rounded-xl border border-white/10 bg-white/[0.04] p-3">
        <div className="mb-3 flex items-center gap-3">
          <span className="grid h-9 w-9 place-items-center rounded-full bg-gradient-to-br from-wave-orange to-wave-violet text-sm font-bold text-white">
            {session.username.slice(0, 1).toUpperCase()}
          </span>
          <span className="min-w-0 flex-1 truncate text-sm font-bold">
            {session.username}
          </span>
        </div>
        <button
          type="button"
          onClick={onLogout}
          className="w-full rounded-lg border border-white/10 px-3 py-2 text-sm font-bold text-neutral-300 transition hover:bg-white/[0.04] hover:text-white"
        >
          Log out
        </button>
      </div>
    </aside>
  );
}

function DesktopNowPlayingRail({ onOpen }: { onOpen: () => void }) {
  const { current, isPlaying, currentTime, duration, toggle, next } = usePlayer();
  if (!current) {
    return (
      <aside className="hidden border-l border-wave-pink/10 px-5 py-6 xl:block">
        <div className="sticky top-6 rounded-xl border border-white/10 bg-white/[0.035] p-5 text-center text-sm font-semibold text-neutral-500">
          Музыка появится здесь, когда начнется воспроизведение.
        </div>
      </aside>
    );
  }
  const displayDuration = duration || current.duration || 0;
  const progress = displayDuration
    ? Math.min((currentTime / displayDuration) * 100, 100)
    : 0;
  return (
    <aside className="hidden border-l border-wave-pink/10 px-5 py-6 xl:block">
      <div className="sticky top-6">
        <button
          type="button"
          onClick={onOpen}
          className="group w-full rounded-xl border border-white/10 bg-white/[0.04] p-4 text-left shadow-2xl shadow-black/20 transition hover:bg-white/[0.06]"
        >
          <p className="mb-3 text-xs font-bold uppercase tracking-[0.2em] text-neutral-500">
            Сейчас играет
          </p>
          <Cover
            coverArt={current.coverArt}
            size={360}
            placeholderSize={80}
            rounded="rounded-xl"
            className="aspect-square w-full shadow-xl shadow-black/30 ring-1 ring-white/10"
          />
          <h2 className="mt-4 truncate text-xl font-extrabold tracking-tight">
            {current.title}
          </h2>
          <p className="truncate text-sm font-semibold text-neutral-400">{current.artist}</p>
        </button>

        <div className="mt-4 rounded-xl border border-white/10 bg-white/[0.035] p-4">
          <div className="h-1 overflow-hidden rounded-full bg-white/10">
            <div
              className="h-full rounded-full bg-gradient-to-r from-wave-orange to-wave-pink transition-[width] duration-500 ease-out"
              style={{ width: `${progress}%` }}
            />
          </div>
          <div className="mt-2 flex justify-between text-xs font-semibold text-neutral-500">
            <span>{formatTime(currentTime)}</span>
            <span>{formatTime(displayDuration)}</span>
          </div>
          <div className="mt-4 flex items-center justify-center gap-4">
            <button
              type="button"
              aria-label={isPlaying ? "pause" : "play"}
              onClick={toggle}
              className="grid h-12 w-12 place-items-center rounded-full bg-[#f3ecdd] text-neutral-950 shadow-lg shadow-black/20 transition active:scale-95"
            >
              {isPlaying ? (
                <PauseIcon className="h-6 w-6" />
              ) : (
                <PlayIcon className="ml-0.5 h-6 w-6" />
              )}
            </button>
            <button
              type="button"
              aria-label="next"
              onClick={next}
              className="grid h-11 w-11 place-items-center rounded-full border border-white/10 text-neutral-300 transition hover:bg-white/[0.06] active:scale-95"
            >
              <NextIcon className="h-5 w-5" />
            </button>
          </div>
        </div>
      </div>
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
    <PlayerProvider session={session}>
      <NavProvider value={nav}>
        <div className="min-h-dvh">
          <div className="mx-auto grid min-h-dvh w-full max-w-[1500px] lg:grid-cols-[240px_minmax(0,1fr)] xl:grid-cols-[240px_minmax(0,1fr)_340px]">
            <DesktopSidebar activeTab={activeTab} session={session} onLogout={onLogout} />

            <main className="min-w-0 px-5 pb-44 pt-6 md:px-8 lg:pb-10 lg:pt-8 xl:px-10">
              <div className="mx-auto w-full max-w-md md:max-w-3xl lg:max-w-5xl">
                {route.name === "home" && (
                  <header className="mb-6 flex items-center gap-3 lg:hidden">
                    <h1 className="flex-1 text-[28px] font-extrabold tracking-tight">
                      Музыка
                    </h1>
                    <button
                      type="button"
                      onClick={onLogout}
                      className="flex items-center gap-2 rounded-full border border-black/5 bg-white/70 py-1.5 pl-1.5 pr-3.5 backdrop-blur transition active:scale-95 dark:border-white/10 dark:bg-white/5"
                    >
                      <span className="grid h-7 w-7 place-items-center rounded-full bg-gradient-to-br from-wave-orange to-wave-violet text-xs font-bold text-white">
                        {session.username.slice(0, 1).toUpperCase()}
                      </span>
                      <span className="text-sm font-semibold text-neutral-600 dark:text-neutral-300">
                        {session.username}
                      </span>
                    </button>
                  </header>
                )}
                <CurrentScreen route={route} />
              </div>
            </main>

            <DesktopNowPlayingRail onOpen={() => setNpOpen(true)} />
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
