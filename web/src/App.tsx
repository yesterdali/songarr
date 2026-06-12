import { FormEvent, useEffect, useState } from "react";
import {
  clearSession,
  createSession,
  loadSession,
  saveSession,
  validateSession,
  type WaveSession,
} from "./auth";
import { NowPlayingBar, NowPlayingScreen } from "./components";
import { LibraryIcon, PlaylistIcon, SearchIcon, WaveIcon } from "./icons";
import { NavProvider, type Route, type TabName } from "./nav";
import { PlayerProvider } from "./player";
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
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setBusy(true);
    try {
      const session = createSession(username, password);
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
          <div className="mb-4 grid h-16 w-16 place-items-center rounded-2xl bg-gradient-to-br from-wave-orange via-wave-pink to-wave-violet shadow-lg shadow-wave-pink/30">
            <WaveIcon className="h-8 w-8 text-white" />
          </div>
          <h1 className="text-3xl font-extrabold tracking-tight">Твоя волна</h1>
          <p className="mt-1 text-sm font-medium text-neutral-500 dark:text-neutral-400">
            Songarr Wave
          </p>
        </div>
        <form
          className="space-y-4 rounded-3xl border border-black/5 bg-white/70 p-6 shadow-xl shadow-black/5 backdrop-blur-xl dark:border-white/10 dark:bg-white/[0.04]"
          onSubmit={submit}
        >
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
        <div className="min-h-full">
          <div className="mx-auto flex min-h-full max-w-md flex-col px-5 pb-44 pt-6">
            {route.name === "home" && (
              <header className="mb-6 flex items-center gap-3">
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

          {/* Bottom chrome: floating dock with the now-playing bar above the tab bar */}
          <div className="fixed inset-x-0 bottom-0 z-10 px-3 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
            <div className="mx-auto max-w-md overflow-hidden rounded-[1.75rem] border border-black/5 bg-white/85 shadow-2xl shadow-black/15 backdrop-blur-2xl dark:border-white/10 dark:bg-neutral-900/85">
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
