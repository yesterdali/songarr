import { useEffect, useState, type ReactNode } from "react";
import * as api from "./api";
import {
  Cover,
  DownloadAllButton,
  EmptyState,
  Segmented,
  SkeletonCardGrid,
  SkeletonRows,
  SongRow,
} from "./components";
import {
  ChevronLeftIcon,
  DownloadIcon,
  GothicCrossIcon,
  HeartIcon,
  LibraryIcon,
  PlayIcon,
  PlaylistIcon,
  SearchIcon,
} from "./icons";
import { useI18n, type MessageKey } from "./i18n";
import { useNav } from "./nav";
import { useOnlineStatus } from "./online";
import { usePlayer } from "./player";
import type { Album, Artist, Playlist, Song } from "./types";

export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]) {
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

export function Status({ loading, error }: { loading: boolean; error: string | null }) {
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

export function SectionTitle({
  children,
  action,
}: {
  children: ReactNode;
  /** Optional trailing control (e.g. an "All" link) shown to the right. */
  action?: ReactNode;
}) {
  return (
    <div className="mb-3 flex items-center gap-3">
      <h2 className="gothic-rule min-w-0 flex-1 font-sans text-xs font-bold uppercase tracking-[0.2em] text-neutral-400 dark:text-neutral-500">
        <span>{children}</span>
      </h2>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

export function ScreenHeader({ title }: { title: string }) {
  const nav = useNav();
  return (
    <header className="mb-5 flex items-center gap-3 md:mb-7">
      {nav.canGoBack && (
        <button
          type="button"
          aria-label="back"
          onClick={nav.back}
          className="grid h-9 w-9 shrink-0 place-items-center rounded-full border border-black/5 bg-white/70 text-neutral-600 backdrop-blur transition active:scale-90 dark:border-white/10 dark:bg-white/5 dark:text-neutral-300 md:h-11 md:w-11"
        >
          <ChevronLeftIcon className="h-5 w-5 md:h-6 md:w-6" />
        </button>
      )}
      <h1 className="truncate text-2xl font-extrabold tracking-tight md:text-4xl">
        {title}
      </h1>
    </header>
  );
}

export function ArtistRow({ artist }: { artist: Artist }) {
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

export function PlayAllButton({ songs }: { songs: Song[] }) {
  const { t } = useI18n();
  const { playQueue } = usePlayer();
  return (
    <button
      type="button"
      onClick={() => playQueue(songs, 0)}
      disabled={songs.length === 0}
      className="inline-flex items-center gap-2 rounded-full bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-2.5 font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-95 disabled:opacity-50 disabled:shadow-none"
    >
      <PlayIcon className="h-5 w-5" /> {t("listen")}
    </button>
  );
}

export function AlbumCard({ album, className = "w-32 shrink-0" }: { album: Album; className?: string }) {
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
  const { t } = useI18n();
  const online = useOnlineStatus();
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
      if (error instanceof Error && error.message === "offline-no-downloads") {
        setWaveError(t("offlineNoDownloads"));
      } else {
        setWaveError(error instanceof Error ? error.message : t("waveNoTracks"));
      }
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
        className="wave-hero group relative mb-7 aspect-[16/10] w-full overflow-hidden rounded-xl text-left shadow-xl shadow-wave-pink/25 transition-transform active:scale-[0.98] md:aspect-[21/8]"
      >
        <GothicCrossIcon className="absolute inset-0 m-auto h-[78%] w-auto text-black/25 transition-transform duration-700 group-active:scale-105" />
        <span className="absolute right-5 top-5 grid h-14 w-14 place-items-center rounded-full border border-wave-pink/40 bg-black/60 text-[#e9e2d4] shadow-lg backdrop-blur transition group-active:scale-90">
          <PlayIcon className="ml-0.5 h-7 w-7" />
        </span>
        <span className="absolute inset-x-5 bottom-5 block">
          <span className="font-display block text-4xl font-bold tracking-tight text-[#f3ecdd] drop-shadow-md md:text-6xl">
            {t("waveTitle")}
          </span>
          <span className="mt-1 block max-w-[80%] text-sm font-medium text-white/85 md:max-w-md md:text-base">
            {waveBusy ? t("waveStarting") : online ? t("waveSubtitle") : t("offlineWaveFallback")}
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
          <SectionTitle
            action={
              <button
                type="button"
                onClick={() => nav.push({ name: "liked" })}
                className="text-sm font-semibold text-wave-pink transition active:opacity-70"
              >
                {t("all")}
              </button>
            }
          >
            {t("liked")}
          </SectionTitle>
          <Status loading={liked.loading} error={liked.error} />
          <div className="scrollbar-none -mx-5 flex snap-x snap-mandatory gap-6 overflow-x-auto scroll-pl-5 px-5 md:mx-0 md:grid md:grid-cols-2 md:gap-x-8 md:gap-y-1 md:overflow-visible md:px-0">
            {likedPages.map((page, pageIndex) => (
              <div key={pageIndex} className="w-[85%] shrink-0 snap-start md:w-full">
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
        <SectionTitle
          action={
            <button
              type="button"
              onClick={() => nav.setTab("library")}
              className="text-sm font-semibold text-wave-pink transition active:opacity-70"
            >
              {t("all")}
            </button>
          }
        >
          {t("recent")}
        </SectionTitle>
        <Status loading={recent.loading} error={recent.error} />
        <div className="scrollbar-none -mx-5 flex snap-x snap-mandatory gap-3 overflow-x-auto scroll-pl-5 px-5 pb-2 md:mx-0 md:grid md:grid-cols-4 md:gap-5 md:overflow-visible md:px-0 lg:grid-cols-5">
          {recent.data?.map((album) => (
            <AlbumCard key={album.id} album={album} className="w-32 shrink-0 md:w-full" />
          ))}
        </div>
      </section>
    </div>
  );
}

export function SearchView() {
  const { session } = usePlayer();
  const { t } = useI18n();
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
      <ScreenHeader title={t("search")} />
      <div className="relative mb-5">
        <SearchIcon className="pointer-events-none absolute left-4 top-1/2 z-10 h-5 w-5 -translate-y-1/2 text-neutral-400" />
        <input
          autoFocus
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder={t("searchPlaceholder")}
          className="relative w-full rounded-lg border border-black/5 bg-white/80 py-3 pl-11 pr-4 text-base shadow-sm outline-none backdrop-blur transition focus:border-wave-pink focus:ring-2 focus:ring-wave-pink/25 dark:border-white/10 dark:bg-white/5"
        />
      </div>

      {results.error && <Status loading={false} error={results.error} />}
      {query.length < 2 ? (
        <EmptyState icon={SearchIcon} title={t("searchPrompt")} />
      ) : results.loading ? (
        <SkeletonRows />
      ) : results.data &&
        !results.data.artists.length &&
        !results.data.albums.length &&
        !results.data.songs.length ? (
        <EmptyState icon={SearchIcon} title={t("searchEmpty")} />
      ) : null}

      {results.data?.artists.length ? (
        <section className="mb-6">
          <SectionTitle>{t("artists")}</SectionTitle>
          {results.data.artists.map((artist) => (
            <ArtistRow key={artist.id} artist={artist} />
          ))}
        </section>
      ) : null}

      {results.data?.albums.length ? (
        <section className="mb-6">
          <SectionTitle>{t("albums")}</SectionTitle>
          <div className="scrollbar-none -mx-5 flex snap-x gap-3 overflow-x-auto px-5 pb-2 md:mx-0 md:grid md:grid-cols-4 md:gap-5 md:overflow-visible md:px-0">
            {results.data.albums.map((album) => (
              <AlbumCard key={album.id} album={album} className="w-32 shrink-0 md:w-full" />
            ))}
          </div>
        </section>
      ) : null}

      {results.data?.songs.length ? (
        <section>
          <SectionTitle>{t("songs")}</SectionTitle>
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
    label: "playlists",
    icon: PlaylistIcon,
    accent: "from-wave-violet/15 text-wave-violet",
  },
  {
    label: "albums",
    icon: LibraryIcon,
    accent: "from-wave-orange/15 text-wave-orange",
  },
  {
    label: "liked",
    icon: HeartIcon,
    accent: "from-wave-pink/15 text-wave-pink",
  },
  {
    label: "downloaded",
    icon: DownloadIcon,
    accent: "from-wave-orange/15 text-wave-orange",
  },
  {
    label: "imports",
    icon: DownloadIcon,
    accent: "from-wave-pink/15 text-wave-pink",
  },
] as const satisfies readonly {
  label: MessageKey;
  icon: typeof PlaylistIcon;
  accent: string;
}[];

export function LibraryView() {
  const { session } = usePlayer();
  const { t } = useI18n();
  const nav = useNav();
  const artists = useAsync(
    async () => api.repairArtistCovers(session, await api.getArtists(session), 80),
    [session],
  );
  const actions = [
    () => nav.setTab("playlists"),
    () => nav.push({ name: "albums" as const }),
    () => nav.push({ name: "liked" as const }),
    () => nav.push({ name: "downloads" as const }),
    () => nav.push({ name: "imports" as const }),
  ];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={t("library")} />
      <div className="mb-6 grid grid-cols-2 gap-3 md:grid-cols-5">
        {LIBRARY_TILES.map(({ label, icon: Icon, accent }, i) => (
          <button
            key={label}
            type="button"
            onClick={actions[i]}
            className={`flex items-center gap-3 rounded-lg border border-black/5 bg-gradient-to-br to-transparent px-4 py-4 text-left font-semibold backdrop-blur transition active:scale-[0.97] dark:border-white/10 ${accent}`}
          >
            <Icon className="h-5 w-5" />
            <span className="text-neutral-900 dark:text-neutral-100">{t(label)}</span>
          </button>
        ))}
      </div>
      <SectionTitle>{t("artists")}</SectionTitle>
      {artists.error && <Status loading={false} error={artists.error} />}
      {artists.loading ? (
        <SkeletonRows />
      ) : (
        <div className="md:grid md:grid-cols-2 md:gap-x-6 xl:grid-cols-3">
          {artists.data?.map((artist: Artist) => (
            <ArtistRow key={artist.id} artist={artist} />
          ))}
        </div>
      )}
    </div>
  );
}

export function AlbumsView() {
  const { session } = usePlayer();
  const { t } = useI18n();
  const [type, setType] = useState<"newest" | "frequent" | "alphabeticalByName">(
    "newest",
  );
  const albums = useAsync(
    async () =>
      api.repairAlbumCovers(session, await api.getAlbumList(session, type, 200), 80),
    [session, type],
  );
  const filters: { type: typeof type; label: string }[] = [
    { type: "newest", label: t("newest") },
    { type: "frequent", label: t("frequent") },
    { type: "alphabeticalByName", label: t("alphabetic") },
  ];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={t("albums")} />
      <Segmented
        value={type}
        onChange={setType}
        className="mb-5 grid grid-cols-3 gap-2"
        options={filters.map((filter) => ({ value: filter.type, label: filter.label }))}
      />
      {albums.error && <Status loading={false} error={albums.error} />}
      {albums.loading ? (
        <SkeletonCardGrid />
      ) : albums.data && albums.data.length === 0 ? (
        <EmptyState icon={LibraryIcon} title={t("albumsEmpty")} />
      ) : (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4 md:gap-5 lg:grid-cols-5">
          {albums.data?.map((album) => (
            <AlbumCard key={album.id} album={album} className="w-full" />
          ))}
        </div>
      )}
    </div>
  );
}

export function ArtistView({ id, title }: { id: string; title: string }) {
  const { session } = usePlayer();
  const { t } = useI18n();
  const data = useAsync(() => api.getArtist(session, id), [session, id]);
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={data.data?.artist.name ?? title} />
      {data.error && <Status loading={false} error={data.error} />}
      {data.loading ? (
        <SkeletonCardGrid />
      ) : data.data && data.data.albums.length === 0 ? (
        <EmptyState icon={LibraryIcon} title={t("artistNoAlbums")} />
      ) : (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4 md:gap-5 lg:grid-cols-5">
          {data.data?.albums.map((album: Album) => (
            <AlbumCard key={album.id} album={album} className="w-full" />
          ))}
        </div>
      )}
    </div>
  );
}

export function ArtistLookupView({ title }: { title: string }) {
  const { session } = usePlayer();
  const { t } = useI18n();
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
              <SectionTitle>{t("artists")}</SectionTitle>
              {result.data.artists.map((artist) => (
                <ArtistRow key={artist.id} artist={artist} />
              ))}
            </section>
          )}
          {result.data.albums.length > 0 && (
            <section className="mb-6">
              <SectionTitle>{t("albums")}</SectionTitle>
              <div className="grid grid-cols-2 gap-4 md:grid-cols-4 md:gap-5">
                {result.data.albums.map((album) => (
                  <AlbumCard key={album.id} album={album} className="w-full" />
                ))}
              </div>
            </section>
          )}
          {result.data.songs.length > 0 && (
            <section>
              <SectionTitle>{t("songs")}</SectionTitle>
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
                {t("nothingFoundArtist")}
              </p>
            )}
        </>
      )}
    </div>
  );
}

export function AlbumView({ id, title }: { id: string; title: string }) {
  const { session } = usePlayer();
  const { t } = useI18n();
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
          <div className="mb-6 flex items-end gap-4 md:items-center md:gap-6">
            <Cover
              coverArt={data.data.album.coverArt}
              size={300}
              rounded="rounded-lg"
              className="h-28 w-28 shrink-0 shadow-xl shadow-black/20 ring-1 ring-black/5 dark:ring-white/10 md:h-44 md:w-44"
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
                {t("tracksCount", { count: songs.length })}
              </p>
              <div className="flex flex-wrap items-center gap-3">
                <PlayAllButton songs={songs} />
                <DownloadAllButton songs={songs} />
              </div>
            </div>
          </div>
          <div className="md:max-w-3xl">
            {songs.map((song, position) => (
              <SongRow key={song.id} song={song} songs={songs} position={position} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

export function PlaylistsView() {
  const { session } = usePlayer();
  const { t } = useI18n();
  const nav = useNav();
  const data = useAsync(() => api.getPlaylists(session), [session]);
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={t("playlists")} />
      {data.error && <Status loading={false} error={data.error} />}
      {data.loading && <SkeletonRows />}
      {data.data && data.data.length === 0 && (
        <EmptyState icon={PlaylistIcon} title={t("playlistsEmpty")} />
      )}
      <div className="md:grid md:grid-cols-2 md:gap-x-6 xl:grid-cols-3">
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
                {t("tracksCount", { count: playlist.songCount ?? 0 })}
              </span>
            </span>
            <ChevronLeftIcon className="h-4 w-4 rotate-180 text-neutral-400 dark:text-neutral-600" />
          </button>
        ))}
      </div>
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
          <div className="mb-5 flex flex-wrap items-center gap-3">
            <PlayAllButton songs={songs} />
            <DownloadAllButton songs={songs} />
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
  const { t } = useI18n();
  const data = useAsync(() => api.getStarred(session), [session]);
  const songs = data.data?.songs ?? [];
  const albums = data.data?.albums ?? [];
  const artists = data.data?.artists ?? [];
  return (
    <div className="animate-fade-in">
      <ScreenHeader title={t("liked")} />
      {data.error && <Status loading={false} error={data.error} />}
      {data.loading && <SkeletonRows />}
      {data.data && songs.length === 0 && albums.length === 0 && artists.length === 0 && (
        <EmptyState icon={HeartIcon} title={t("likedEmpty")} />
      )}
      {albums.length > 0 && (
        <section className="mb-6">
          <SectionTitle>{t("albums")}</SectionTitle>
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4 md:gap-5 lg:grid-cols-5">
            {albums.map((album) => (
              <AlbumCard key={album.id} album={album} className="w-full" />
            ))}
          </div>
        </section>
      )}
      {artists.length > 0 && (
        <section className="mb-6">
          <SectionTitle>{t("artists")}</SectionTitle>
          <div className="md:grid md:grid-cols-2 md:gap-x-6 xl:grid-cols-3">
            {artists.map((artist) => (
              <ArtistRow key={artist.id} artist={artist} />
            ))}
          </div>
        </section>
      )}
      {songs.length > 0 && (
        <section>
          <SectionTitle>{t("songs")}</SectionTitle>
          {songs.map((song, position) => (
            <SongRow key={song.id} song={song} songs={songs} position={position} />
          ))}
        </section>
      )}
    </div>
  );
}
