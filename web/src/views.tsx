import { useEffect, useState, type ReactNode } from "react";
import * as api from "./api";
import { Cover, SongRow } from "./components";
import {
  ChevronLeftIcon,
  GothicCrossIcon,
  HeartIcon,
  LibraryIcon,
  PlayIcon,
  PlaylistIcon,
  SearchIcon,
} from "./icons";
import { useNav } from "./nav";
import { usePlayer } from "./player";
import type { Album, Artist, Playlist, Song } from "./types";

function useAsync<T>(fn: () => Promise<T>, deps: unknown[]) {
  const [state, setState] = useState<{
    data: T | null;
    loading: boolean;
    error: string | null;
  }>({ data: null, loading: true, error: null });
  useEffect(() => {
    let cancelled = false;
    setState({ data: null, loading: true, error: null });
    fn()
      .then((data) => {
        if (!cancelled) setState({ data, loading: false, error: null });
      })
      .catch((error: unknown) => {
        if (!cancelled)
          setState({
            data: null,
            loading: false,
            error: error instanceof Error ? error.message : "Failed to load",
          });
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}

function Status({ loading, error }: { loading: boolean; error: string | null }) {
  if (loading)
    return (
      <div className="flex justify-center py-10">
        <div className="h-7 w-7 animate-spin rounded-full border-[3px] border-wave-pink/25 border-t-wave-pink" />
      </div>
    );
  if (error)
    return (
      <p className="mx-auto my-6 max-w-sm rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-center text-sm font-medium text-red-600 dark:text-red-300">
        {error}
      </p>
    );
  return null;
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="gothic-rule mb-3 font-sans text-xs font-bold uppercase tracking-[0.2em] text-neutral-400 dark:text-neutral-500">
      <span>{children}</span>
    </h2>
  );
}

function ScreenHeader({ title }: { title: string }) {
  const nav = useNav();
  return (
    <header className="mb-5 flex items-center gap-3">
      {nav.canGoBack && (
        <button
          type="button"
          aria-label="back"
          onClick={nav.back}
          className="grid h-9 w-9 shrink-0 place-items-center rounded-full border border-black/5 bg-white/70 text-neutral-600 backdrop-blur transition active:scale-90 dark:border-white/10 dark:bg-white/5 dark:text-neutral-300"
        >
          <ChevronLeftIcon className="h-5 w-5" />
        </button>
      )}
      <h1 className="truncate text-2xl font-extrabold tracking-tight">{title}</h1>
    </header>
  );
}

function ArtistRow({ artist }: { artist: Artist }) {
  const nav = useNav();
  return (
    <button
      type="button"
      onClick={() => nav.push({ name: "artist", id: artist.id, title: artist.name })}
      className="-mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-xl px-2 py-2 text-left transition-colors hover:bg-black/[0.04] active:bg-black/[0.06] dark:hover:bg-white/[0.04] dark:active:bg-white/[0.07]"
    >
      {artist.coverArt ? (
        <Cover
          coverArt={artist.coverArt}
          size={96}
          rounded="rounded-full"
          className="h-11 w-11 shrink-0 shadow-md shadow-black/10 ring-1 ring-black/5 dark:ring-white/10"
        />
      ) : (
        <span className="grid h-11 w-11 shrink-0 place-items-center rounded-full bg-gradient-to-br from-wave-orange/80 to-wave-violet/80 text-base font-bold text-white">
          {artist.name.slice(0, 1).toUpperCase()}
        </span>
      )}
      <span className="min-w-0 flex-1 truncate text-sm font-semibold">{artist.name}</span>
      <ChevronLeftIcon className="h-4 w-4 rotate-180 text-neutral-400 dark:text-neutral-600" />
    </button>
  );
}

function PlayAllButton({ songs }: { songs: Song[] }) {
  const { playQueue } = usePlayer();
  return (
    <button
      type="button"
      onClick={() => playQueue(songs, 0)}
      disabled={songs.length === 0}
      className="inline-flex items-center gap-2 rounded-full bg-gradient-to-r from-wave-orange to-wave-pink px-5 py-2.5 font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-95 disabled:opacity-50 disabled:shadow-none"
    >
      <PlayIcon className="h-5 w-5" /> Слушать
    </button>
  );
}

function AlbumCard({ album, className = "w-32 shrink-0" }: { album: Album; className?: string }) {
  const nav = useNav();
  return (
    <button
      type="button"
      onClick={() => nav.push({ name: "album", id: album.id, title: album.name })}
      className={`${className} group snap-start text-left transition-transform active:scale-[0.97]`}
    >
      <Cover
        coverArt={album.coverArt}
        size={200}
        rounded="rounded-lg"
        className="aspect-square w-full shadow-lg shadow-black/10 ring-1 ring-black/5 dark:ring-white/10"
      />
      <p className="mt-2 truncate text-sm font-semibold">{album.name}</p>
      <p className="truncate text-xs text-neutral-500 dark:text-neutral-400">
        {album.artist}
      </p>
    </button>
  );
}

export function HomeView() {
  const { session, startWave } = usePlayer();
  const nav = useNav();
  const [waveBusy, setWaveBusy] = useState(false);
  const [waveError, setWaveError] = useState<string | null>(null);
  const liked = useAsync(() => api.getStarred(session), [session]);
  const recent = useAsync(
    async () => api.repairAlbumCovers(session, await api.getAlbumList(session, "newest")),
    [session],
  );

  // YT-Music-style quick picks: liked tracks in swipeable pages of 4 rows.
  const likedSongs = liked.data?.songs ?? [];
  const likedPages: Song[][] = [];
  for (let i = 0; i < Math.min(likedSongs.length, 24); i += 4) {
    likedPages.push(likedSongs.slice(i, i + 4));
  }

  async function playWave() {
    setWaveBusy(true);
    setWaveError(null);
    try {
      await startWave();
    } catch (error) {
      setWaveError(error instanceof Error ? error.message : "Wave failed to start");
    } finally {
      setWaveBusy(false);
    }
  }

  return (
    <div className="animate-fade-in">
      <button
        type="button"
        onClick={playWave}
        disabled={waveBusy}
        className="wave-hero group relative mb-7 aspect-[16/10] w-full overflow-hidden rounded-xl text-left shadow-xl shadow-wave-pink/25 transition-transform active:scale-[0.98]"
      >
        <GothicCrossIcon className="absolute inset-0 m-auto h-[78%] w-auto text-black/25 transition-transform duration-700 group-active:scale-105" />
        <span className="absolute right-5 top-5 grid h-14 w-14 place-items-center rounded-full border border-wave-pink/40 bg-black/60 text-[#e9e2d4] shadow-lg backdrop-blur transition group-active:scale-90">
          <PlayIcon className="ml-0.5 h-7 w-7" />
        </span>
        <span className="absolute inset-x-5 bottom-5 block">
          <span className="font-display block text-4xl font-bold tracking-tight text-[#f3ecdd] drop-shadow-md">
            Твоя волна
          </span>
          <span className="mt-1 block max-w-[80%] text-sm font-medium text-white/85">
            {waveBusy ? "Запускаю…" : "бесконечный поток музыки, подобранной для тебя"}
          </span>
        </span>
      </button>
      {waveError && (
        <p className="-mt-4 mb-5 rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm font-medium text-red-600 dark:text-red-300">
          {waveError}
        </p>
      )}

      {(liked.loading || likedSongs.length > 0) && (
        <section className="mb-7">
          <div className="mb-3 flex items-baseline justify-between">
            <h2 className="text-lg font-bold tracking-tight">Любимые треки</h2>
            <button
              type="button"
              onClick={() => nav.push({ name: "liked" })}
              className="text-sm font-semibold text-wave-pink transition active:opacity-70"
            >
              Всё
            </button>
          </div>
          <Status loading={liked.loading} error={liked.error} />
          <div className="scrollbar-none -mx-5 flex snap-x snap-mandatory gap-6 overflow-x-auto scroll-pl-5 px-5">
            {likedPages.map((page, pageIndex) => (
              <div key={pageIndex} className="w-[85%] shrink-0 snap-start">
                {page.map((song, i) => (
                  <SongRow
                    key={song.id}
                    song={song}
                    songs={likedSongs}
                    position={pageIndex * 4 + i}
                  />
                ))}
              </div>
            ))}
          </div>
        </section>
      )}

      <section>
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="text-lg font-bold tracking-tight">Недавнее</h2>
          <button
            type="button"
            onClick={() => nav.setTab("library")}
            className="text-sm font-semibold text-wave-pink transition active:opacity-70"
          >
            Всё
          </button>
        </div>
        <Status loading={recent.loading} error={recent.error} />
        <div className="scrollbar-none -mx-5 flex snap-x snap-mandatory gap-3 overflow-x-auto scroll-pl-5 px-5 pb-2">
          {recent.data?.map((album) => (
            <AlbumCard key={album.id} album={album} />
          ))}
        </div>
      </section>
    </div>
  );
}

export function SearchView() {
  const { session } = usePlayer();
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");

  useEffect(() => {
    const id = setTimeout(() => setQuery(text.trim()), 300);
    return () => clearTimeout(id);
  }, [text]);

  const results = useAsync(
    () =>
      query.length >= 2
        ? api.search(session, query)
        : Promise.resolve({ songs: [], albums: [], artists: [] }),
    [session, query],
  );

  return (
    <div className="animate-fade-in">
      <ScreenHeader title="Поиск" />
      <div className="relative mb-5">
        <SearchIcon className="pointer-events-none absolute left-4 top-1/2 h-5 w-5 -translate-y-1/2 text-neutral-400" />
        <input
          autoFocus
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="Песни, артисты, альбомы"
          className="w-full rounded-lg border border-black/5 bg-white/80 py-3 pl-11 pr-4 text-base shadow-sm outline-none backdrop-blur transition focus:border-wave-pink focus:ring-2 focus:ring-wave-pink/25 dark:border-white/10 dark:bg-white/5"
        />
      </div>

      {query.length >= 2 && <Status loading={results.loading} error={results.error} />}

      {results.data?.artists.length ? (
        <section className="mb-6">
          <SectionTitle>Артисты</SectionTitle>
          {results.data.artists.map((artist) => (
            <ArtistRow key={artist.id} artist={artist} />
          ))}
        </section>
      ) : null}

      {results.data?.albums.length ? (
        <section className="mb-6">
          <SectionTitle>Альбомы</SectionTitle>
          <div className="scrollbar-none -mx-5 flex snap-x gap-3 overflow-x-auto px-5 pb-2">
            {results.data.albums.map((album) => (
              <AlbumCard key={album.id} album={album} />
            ))}
          </div>
        </section>
      ) : null}

      {results.data?.songs.length ? (
        <section>
          <SectionTitle>Песни</SectionTitle>
          {results.data.songs.map((song, position) => (
            <SongRow key={song.id} song={song} songs={results.data!.songs} position={position} />
          ))}
        </section>
      ) : null}
    </div>
  );
}

const LIBRARY_TILES = [
  {
    label: "Плейлисты",
    icon: PlaylistIcon,
    accent: "from-wave-violet/15 text-wave-violet",
  },
  {
    label: "Альбомы",
    icon: LibraryIcon,
    accent: "from-wave-orange/15 text-wave-orange",
  },
  {
    label: "Любимое",
    icon: HeartIcon,
    accent: "from-wave-pink/15 text-wave-pink",
  },
] as const;

export function LibraryView() {
  const { session } = usePlayer();
  const nav = useNav();
  const artists = useAsync(
    async () => api.repairArtistCovers(session, await api.getArtists(session), 80),
    [session],
  );
  const actions = [
    () => nav.setTab("playlists"),
    () => nav.push({ name: "albums" as const }),
    () => nav.push({ name: "liked" as const }),
  ];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title="Библиотека" />
      <div className="mb-6 grid grid-cols-2 gap-3">
        {LIBRARY_TILES.map(({ label, icon: Icon, accent }, i) => (
          <button
            key={label}
            type="button"
            onClick={actions[i]}
            className={`flex items-center gap-3 rounded-lg border border-black/5 bg-gradient-to-br to-transparent px-4 py-4 text-left font-semibold backdrop-blur transition active:scale-[0.97] dark:border-white/10 ${accent}`}
          >
            <Icon className="h-5 w-5" />
            <span className="text-neutral-900 dark:text-neutral-100">{label}</span>
          </button>
        ))}
      </div>
      <SectionTitle>Артисты</SectionTitle>
      <Status loading={artists.loading} error={artists.error} />
      {artists.data?.map((artist: Artist) => (
        <ArtistRow key={artist.id} artist={artist} />
      ))}
    </div>
  );
}

export function AlbumsView() {
  const { session } = usePlayer();
  const [type, setType] = useState<"newest" | "frequent" | "alphabeticalByName">(
    "newest",
  );
  const albums = useAsync(
    async () =>
      api.repairAlbumCovers(session, await api.getAlbumList(session, type, 200), 80),
    [session, type],
  );
  const filters: { type: typeof type; label: string }[] = [
    { type: "newest", label: "Новые" },
    { type: "frequent", label: "Частые" },
    { type: "alphabeticalByName", label: "А-Я" },
  ];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title="Альбомы" />
      <div className="mb-5 grid grid-cols-3 gap-1 rounded-lg border border-black/5 bg-black/[0.04] p-1 backdrop-blur dark:border-white/5 dark:bg-white/5">
        {filters.map((filter) => (
          <button
            key={filter.type}
            type="button"
            onClick={() => setType(filter.type)}
            className={`rounded-xl px-3 py-2 text-sm font-bold transition ${
              type === filter.type
                ? "bg-white text-neutral-950 shadow-md dark:bg-neutral-700 dark:text-white"
                : "text-neutral-500 active:text-neutral-700 dark:text-neutral-400"
            }`}
          >
            {filter.label}
          </button>
        ))}
      </div>
      <Status loading={albums.loading} error={albums.error} />
      <div className="grid grid-cols-2 gap-4">
        {albums.data?.map((album) => (
          <AlbumCard key={album.id} album={album} className="w-full" />
        ))}
      </div>
    </div>
  );
}

export function ArtistView({ id, title }: { id: string; title: string }) {
  const { session } = usePlayer();
  const data = useAsync(() => api.getArtist(session, id), [session, id]);
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={data.data?.artist.name ?? title} />
      <Status loading={data.loading} error={data.error} />
      <div className="grid grid-cols-2 gap-4">
        {data.data?.albums.map((album: Album) => (
          <AlbumCard key={album.id} album={album} className="w-full" />
        ))}
      </div>
    </div>
  );
}

export function ArtistLookupView({ title }: { title: string }) {
  const { session } = usePlayer();
  const result = useAsync(() => api.search(session, title), [session, title]);
  const exact =
    result.data?.artists.find(
      (artist) => artist.name.localeCompare(title, undefined, { sensitivity: "accent" }) === 0,
    ) ?? result.data?.artists[0];
  if (exact) {
    return <ArtistView id={exact.id} title={exact.name} />;
  }
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={title} />
      <Status loading={result.loading} error={result.error} />
      {result.data && (
        <>
          {result.data.artists.length > 0 && (
            <section className="mb-6">
              <SectionTitle>Артисты</SectionTitle>
              {result.data.artists.map((artist) => (
                <ArtistRow key={artist.id} artist={artist} />
              ))}
            </section>
          )}
          {result.data.albums.length > 0 && (
            <section className="mb-6">
              <SectionTitle>Альбомы</SectionTitle>
              <div className="grid grid-cols-2 gap-4">
                {result.data.albums.map((album) => (
                  <AlbumCard key={album.id} album={album} className="w-full" />
                ))}
              </div>
            </section>
          )}
          {result.data.songs.length > 0 && (
            <section>
              <SectionTitle>Песни</SectionTitle>
              {result.data.songs.map((song, position) => (
                <SongRow
                  key={`${song.id}-${position}`}
                  song={song}
                  songs={result.data!.songs}
                  position={position}
                />
              ))}
            </section>
          )}
          {result.data.artists.length === 0 &&
            result.data.albums.length === 0 &&
            result.data.songs.length === 0 && (
              <p className="py-10 text-center text-sm text-neutral-500">
                Ничего не нашлось по имени артиста.
              </p>
            )}
        </>
      )}
    </div>
  );
}

export function AlbumView({ id, title }: { id: string; title: string }) {
  const { session } = usePlayer();
  const nav = useNav();
  const data = useAsync(() => api.getAlbum(session, id), [session, id]);
  const songs = data.data?.songs ?? [];
  const album = data.data?.album;
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={album?.name ?? title} />
      <Status loading={data.loading} error={data.error} />
      {data.data && (
        <>
          <div className="mb-6 flex items-end gap-4">
            <Cover
              coverArt={data.data.album.coverArt}
              size={300}
              rounded="rounded-lg"
              className="h-28 w-28 shrink-0 shadow-xl shadow-black/20 ring-1 ring-black/5 dark:ring-white/10"
            />
            <div className="min-w-0 flex-1">
              <button
                type="button"
                onClick={() =>
                  album?.artistId
                    ? nav.push({
                        name: "artist",
                        id: album.artistId,
                        title: album.artist,
                      })
                    : nav.push({ name: "artistLookup", title: album?.artist ?? "" })
                }
                className="block max-w-full truncate text-left font-semibold text-wave-pink active:opacity-70"
              >
                {data.data.album.artist}
              </button>
              <p className="mb-3 text-sm text-neutral-500 dark:text-neutral-400">
                {songs.length} треков
              </p>
              <PlayAllButton songs={songs} />
            </div>
          </div>
          {songs.map((song, position) => (
            <SongRow key={song.id} song={song} songs={songs} position={position} />
          ))}
        </>
      )}
    </div>
  );
}

export function PlaylistsView() {
  const { session } = usePlayer();
  const nav = useNav();
  const data = useAsync(() => api.getPlaylists(session), [session]);
  return (
    <div className="animate-fade-in">
      <ScreenHeader title="Плейлисты" />
      <Status loading={data.loading} error={data.error} />
      {data.data?.map((playlist: Playlist) => (
        <button
          key={playlist.id}
          type="button"
          onClick={() =>
            nav.push({ name: "playlist", id: playlist.id, title: playlist.name })
          }
          className="-mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-xl px-2 py-2 text-left transition-colors hover:bg-black/[0.04] active:bg-black/[0.06] dark:hover:bg-white/[0.04] dark:active:bg-white/[0.07]"
        >
          <Cover
            coverArt={playlist.coverArt}
            size={120}
            className="h-12 w-12 shrink-0 shadow-md ring-1 ring-black/5 dark:ring-white/10"
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-semibold">{playlist.name}</span>
            <span className="block truncate text-xs text-neutral-500 dark:text-neutral-400">
              {playlist.songCount ?? 0} треков
            </span>
          </span>
          <ChevronLeftIcon className="h-4 w-4 rotate-180 text-neutral-400 dark:text-neutral-600" />
        </button>
      ))}
    </div>
  );
}

export function PlaylistView({ id, title }: { id: string; title: string }) {
  const { session } = usePlayer();
  const data = useAsync(() => api.getPlaylist(session, id), [session, id]);
  const songs = data.data?.songs ?? [];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={data.data?.playlist.name ?? title} />
      <Status loading={data.loading} error={data.error} />
      {data.data && (
        <>
          <div className="mb-5">
            <PlayAllButton songs={songs} />
          </div>
          {songs.map((song, position) => (
            <SongRow key={`${song.id}-${position}`} song={song} songs={songs} position={position} />
          ))}
        </>
      )}
    </div>
  );
}

export function LikedView() {
  const { session } = usePlayer();
  const data = useAsync(() => api.getStarred(session), [session]);
  const songs = data.data?.songs ?? [];
  const albums = data.data?.albums ?? [];
  const artists = data.data?.artists ?? [];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title="Любимое" />
      <Status loading={data.loading} error={data.error} />
      {data.data && songs.length === 0 && albums.length === 0 && artists.length === 0 && (
        <div className="flex flex-col items-center gap-3 py-12 text-center">
          <span className="grid h-14 w-14 place-items-center rounded-full bg-wave-pink/10 text-wave-pink">
            <HeartIcon className="h-7 w-7" />
          </span>
          <p className="max-w-60 text-sm text-neutral-500 dark:text-neutral-400">
            Лайкни трек, альбом или артиста — он появится здесь.
          </p>
        </div>
      )}
      {albums.length > 0 && (
        <section className="mb-6">
          <SectionTitle>Альбомы</SectionTitle>
          <div className="grid grid-cols-2 gap-4">
            {albums.map((album) => (
              <AlbumCard key={album.id} album={album} className="w-full" />
            ))}
          </div>
        </section>
      )}
      {artists.length > 0 && (
        <section className="mb-6">
          <SectionTitle>Артисты</SectionTitle>
          {artists.map((artist) => (
            <ArtistRow key={artist.id} artist={artist} />
          ))}
        </section>
      )}
      {songs.length > 0 && (
        <section>
          <SectionTitle>Песни</SectionTitle>
          {songs.map((song, position) => (
            <SongRow key={song.id} song={song} songs={songs} position={position} />
          ))}
        </section>
      )}
    </div>
  );
}
