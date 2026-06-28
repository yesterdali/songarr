import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { coverUrl, streamUrl } from "./api";
import type { WaveSession } from "./auth";
import { getDownloadQuality } from "./quality";
import type { Song } from "./types";

// ---- IndexedDB layer: audio blobs stored for offline / instant playback ----

const DB_NAME = "songarr-downloads";
const DB_VERSION = 2;
const STORE = "tracks";

export type DownloadRecord = {
  id: string;
  blob: Blob;
  song: Song;
  savedAt: number;
  coverBlob?: Blob;
};

export type DownloadedSong = Song & { savedAt: number; downloaded: true };

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE, { keyPath: "id" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  return dbPromise;
}

function run<T>(
  mode: IDBTransactionMode,
  fn: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return openDb().then(
    (db) =>
      new Promise<T>((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        const request = fn(tx.objectStore(STORE));
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      }),
  );
}

async function loadDownloadedIds(): Promise<string[]> {
  try {
    const keys = await run<IDBValidKey[]>("readonly", (store) => store.getAllKeys());
    return keys.map(String);
  } catch {
    return [];
  }
}

async function getDownloadRecord(id: string): Promise<DownloadRecord | undefined> {
  try {
    return await run<DownloadRecord | undefined>("readonly", (store) => store.get(id));
  } catch {
    return undefined;
  }
}

async function getDownloadRecords(): Promise<DownloadRecord[]> {
  try {
    return await run<DownloadRecord[]>("readonly", (store) => store.getAll());
  } catch {
    return [];
  }
}

async function deleteDownload(id: string): Promise<void> {
  await run("readwrite", (store) => store.delete(id));
}

async function putDownloadRecord(record: DownloadRecord): Promise<void> {
  await run("readwrite", (store) => store.put(record));
}

async function fetchCoverBlob(session: WaveSession, song: Song): Promise<Blob | undefined> {
  const url = coverUrl(session, song.coverArt, 600);
  if (!url) return undefined;
  try {
    const response = await fetch(url, { headers: { Accept: "image/*" } });
    if (!response.ok) return undefined;
    const blob = await response.blob();
    return blob.size > 0 ? blob : undefined;
  } catch {
    return undefined;
  }
}

async function saveDownload(session: WaveSession, song: Song): Promise<void> {
  const url = song.streamUrl ?? streamUrl(session, song.id, getDownloadQuality());
  const response = await fetch(url, { headers: { Accept: "audio/*" } });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const blob = await response.blob();
  if (blob.size === 0) throw new Error("empty audio");
  const coverBlob = await fetchCoverBlob(session, song);
  const record: DownloadRecord = { id: song.id, blob, song, savedAt: Date.now(), coverBlob };
  await putDownloadRecord(record);
}

export const __downloadStoreForTests = {
  putDownloadRecord,
  getDownloadRecord,
  getDownloadRecords,
  deleteDownload,
  loadDownloadedIds,
  resetDbPromise() {
    dbPromise = null;
  },
};

// ---- React provider ----

type DownloadsValue = {
  isDownloaded: (id: string) => boolean;
  isDownloading: (id: string) => boolean;
  /** How many of these ids are downloaded (for album/playlist progress). */
  downloadedCount: (ids: string[]) => number;
  downloadedSongs: DownloadedSong[];
  download: (song: Song) => Promise<void>;
  downloadAlbum: (songs: Song[]) => Promise<void>;
  remove: (id: string) => Promise<void>;
  /** Toggle a single track: download if absent, remove if present. */
  toggle: (song: Song) => void;
  /** Object URL for a downloaded track, or null. Stable identity. */
  getPlayableUrl: (id: string) => Promise<string | null>;
  /** Object URL for a downloaded cover, or null. Stable identity. */
  getCoverUrl: (id: string) => Promise<string | null>;
  downloadError: (id: string) => string | null;
  refresh: () => Promise<void>;
};

const DownloadsContext = createContext<DownloadsValue | null>(null);

export function useDownloads(): DownloadsValue {
  const value = useContext(DownloadsContext);
  if (!value) throw new Error("useDownloads used outside DownloadsProvider");
  return value;
}

export function DownloadsProvider({
  session,
  children,
}: {
  session: WaveSession;
  children: ReactNode;
}) {
  const [downloaded, setDownloaded] = useState<Set<string>>(new Set());
  const [downloadedSongs, setDownloadedSongs] = useState<DownloadedSong[]>([]);
  const [downloading, setDownloading] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Map<string, string>>(new Map());
  // Latest sets for callbacks that must not churn identity on every change.
  const downloadedRef = useRef(downloaded);
  downloadedRef.current = downloaded;
  const downloadingRef = useRef(downloading);
  downloadingRef.current = downloading;
  // Lazily-created blob: URLs for playback; revoked on remove/unmount.
  const objectUrls = useRef<Map<string, string>>(new Map());
  const coverUrls = useRef<Map<string, string>>(new Map());

  const refresh = useCallback(async () => {
    const records = await getDownloadRecords();
    setDownloaded(new Set(records.map((record) => record.id)));
    setDownloadedSongs(
      records
        .map((record) => ({
          ...record.song,
          savedAt: record.savedAt ?? 0,
          downloaded: true as const,
        }))
        .sort((a, b) => b.savedAt - a.savedAt),
    );
  }, []);

  useEffect(() => {
    let cancelled = false;
    getDownloadRecords().then((records) => {
      if (cancelled) return;
      setDownloaded(new Set(records.map((record) => record.id)));
      setDownloadedSongs(
        records
          .map((record) => ({ ...record.song, savedAt: record.savedAt ?? 0, downloaded: true as const }))
          .sort((a, b) => b.savedAt - a.savedAt),
      );
    });
    loadDownloadedIds().then((ids) => {
      if (!cancelled && ids.length > 0) setDownloaded((prev) => new Set([...prev, ...ids]));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const urls = objectUrls.current;
    return () => {
      for (const url of urls.values()) URL.revokeObjectURL(url);
      urls.clear();
      for (const url of coverUrls.current.values()) URL.revokeObjectURL(url);
      coverUrls.current.clear();
    };
  }, []);

  const download = useCallback(
    async (song: Song) => {
      if (downloadedRef.current.has(song.id) || downloadingRef.current.has(song.id)) return;
      setDownloading((prev) => new Set(prev).add(song.id));
      setErrors((prev) => {
        const next = new Map(prev);
        next.delete(song.id);
        return next;
      });
      try {
        await saveDownload(session, song);
        setDownloaded((prev) => new Set(prev).add(song.id));
        await refresh();
      } catch (error) {
        console.warn("download failed", song.id, error);
        setErrors((prev) =>
          new Map(prev).set(song.id, error instanceof Error ? error.message : "Download failed"),
        );
      } finally {
        setDownloading((prev) => {
          const next = new Set(prev);
          next.delete(song.id);
          return next;
        });
      }
    },
    [refresh, session],
  );

  const downloadAlbum = useCallback(
    async (songs: Song[]) => {
      // Sequential: the server caps concurrent virtual streams, and it keeps
      // the per-track spinners advancing in order.
      for (const song of songs) {
        if (downloadedRef.current.has(song.id)) continue;
        await download(song);
      }
    },
    [download],
  );

  const remove = useCallback(async (id: string) => {
    await deleteDownload(id);
    const url = objectUrls.current.get(id);
    if (url) {
      URL.revokeObjectURL(url);
      objectUrls.current.delete(id);
    }
    const cover = coverUrls.current.get(id);
    if (cover) {
      URL.revokeObjectURL(cover);
      coverUrls.current.delete(id);
    }
    setDownloaded((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
    setDownloadedSongs((prev) => prev.filter((song) => song.id !== id));
  }, []);

  const toggle = useCallback(
    (song: Song) => {
      if (downloadedRef.current.has(song.id)) void remove(song.id);
      else void download(song);
    },
    [download, remove],
  );

  // Stable: reads IndexedDB directly, so the player can depend on it without
  // re-running playback effects when the downloaded set changes.
  const getPlayableUrl = useCallback(async (id: string): Promise<string | null> => {
    const cached = objectUrls.current.get(id);
    if (cached) return cached;
    const record = await getDownloadRecord(id);
    if (!record) return null;
    const url = URL.createObjectURL(record.blob);
    objectUrls.current.set(id, url);
    return url;
  }, []);

  const getCoverUrl = useCallback(async (id: string): Promise<string | null> => {
    const cached = coverUrls.current.get(id);
    if (cached) return cached;
    const record = await getDownloadRecord(id);
    if (!record?.coverBlob) return null;
    const url = URL.createObjectURL(record.coverBlob);
    coverUrls.current.set(id, url);
    return url;
  }, []);

  const isDownloaded = useCallback((id: string) => downloaded.has(id), [downloaded]);
  const isDownloading = useCallback((id: string) => downloading.has(id), [downloading]);
  const downloadError = useCallback((id: string) => errors.get(id) ?? null, [errors]);
  const downloadedCount = useCallback(
    (ids: string[]) => ids.reduce((n, id) => n + (downloaded.has(id) ? 1 : 0), 0),
    [downloaded],
  );

  const value = useMemo<DownloadsValue>(
    () => ({
      isDownloaded,
      isDownloading,
      downloadedCount,
      downloadedSongs,
      download,
      downloadAlbum,
      remove,
      toggle,
      getPlayableUrl,
      getCoverUrl,
      downloadError,
      refresh,
    }),
    [
      isDownloaded,
      isDownloading,
      downloadedCount,
      downloadedSongs,
      download,
      downloadAlbum,
      remove,
      toggle,
      getPlayableUrl,
      getCoverUrl,
      downloadError,
      refresh,
    ],
  );

  return <DownloadsContext.Provider value={value}>{children}</DownloadsContext.Provider>;
}
