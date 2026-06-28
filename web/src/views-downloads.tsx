import { Cover } from "./components";
import { useDownloads, type DownloadedSong } from "./downloads";
import { DownloadIcon, PlayIcon } from "./icons";
import { useI18n } from "./i18n";
import { usePlayer } from "./player";
import { ScreenHeader, SectionTitle } from "./views";

function shuffle<T>(items: T[]): T[] {
  const copy = [...items];
  for (let i = copy.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

function groupDownloaded(songs: DownloadedSong[]): [string, DownloadedSong[]][] {
  const groups = new Map<string, DownloadedSong[]>();
  for (const song of songs) {
    const key = song.album ? `${song.artist} - ${song.album}` : song.artist;
    groups.set(key, [...(groups.get(key) ?? []), song]);
  }
  return [...groups.entries()];
}

export function DownloadsView() {
  const { t } = useI18n();
  const { downloadedSongs, remove } = useDownloads();
  const { playQueue } = usePlayer();
  const groups = groupDownloaded(downloadedSongs);

  return (
    <div className="animate-fade-in">
      <ScreenHeader title={t("downloaded")} />
      <div className="mb-5 flex flex-wrap gap-3">
        <button
          type="button"
          disabled={downloadedSongs.length === 0}
          onClick={() => playQueue(shuffle(downloadedSongs), 0)}
          className="inline-flex items-center gap-2 rounded-full bg-gradient-to-r from-wave-orange via-wave-pink to-wave-violet px-5 py-2.5 font-bold text-white shadow-lg shadow-wave-pink/30 transition active:scale-95 disabled:opacity-50 disabled:shadow-none"
        >
          <PlayIcon className="h-5 w-5" />
          {t("shuffleDownloads")}
        </button>
      </div>

      {downloadedSongs.length === 0 ? (
        <div className="flex flex-col items-center gap-3 py-12 text-center">
          <span className="grid h-14 w-14 place-items-center rounded-full bg-wave-pink/10 text-wave-pink">
            <DownloadIcon className="h-7 w-7" />
          </span>
          <p className="max-w-64 text-sm text-neutral-500 dark:text-neutral-400">
            {t("noDownloads")}
          </p>
        </div>
      ) : (
        <div className="space-y-7">
          {groups.map(([group, songs]) => (
            <section key={group}>
              <SectionTitle>{group}</SectionTitle>
              <div className="md:grid md:grid-cols-2 md:gap-x-6">
                {songs.map((song, position) => (
                  <div
                    key={song.id}
                    className="-mx-2 flex items-center gap-3 rounded-xl px-2 py-2 transition-colors hover:bg-black/[0.04] dark:hover:bg-white/[0.04]"
                  >
                    <button
                      type="button"
                      onClick={() => playQueue(songs, position)}
                      className="flex min-w-0 flex-1 items-center gap-3 text-left"
                    >
                      <Cover
                        coverArt={song.coverArt}
                        downloadId={song.id}
                        size={80}
                        className="h-11 w-11 shrink-0"
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm font-semibold">
                          {song.title}
                        </span>
                        <span className="block truncate text-xs text-neutral-500 dark:text-neutral-400">
                          {song.artist}
                        </span>
                      </span>
                    </button>
                    <button
                      type="button"
                      onClick={() => remove(song.id)}
                      className="rounded-full border border-white/10 px-3 py-1.5 text-xs font-bold text-neutral-500 transition hover:text-red-400 active:scale-95"
                    >
                      {t("removeDownload")}
                    </button>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
