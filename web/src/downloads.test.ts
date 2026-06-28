import { afterEach, describe, expect, it, vi } from "vitest";

import { __downloadStoreForTests, type DownloadRecord } from "./downloads";

type FakeRequest<T> = IDBRequest<T> & {
  result: T;
  error: DOMException | null;
  onsuccess: ((this: IDBRequest<T>, ev: Event) => unknown) | null;
  onerror: ((this: IDBRequest<T>, ev: Event) => unknown) | null;
};

function successRequest<T>(result: T): IDBRequest<T> {
  const request = {
    result,
    error: null,
    onsuccess: null,
    onerror: null,
  } as FakeRequest<T>;
  queueMicrotask(() => request.onsuccess?.call(request, new Event("success")));
  return request;
}

function installIndexedDbMock() {
  const records = new Map<string, DownloadRecord>();
  let created = false;

  const store = {
    getAllKeys: () => successRequest([...records.keys()]),
    get: (id: IDBValidKey) => successRequest(records.get(String(id))),
    getAll: () => successRequest([...records.values()]),
    delete: (id: IDBValidKey) => {
      records.delete(String(id));
      return successRequest(undefined);
    },
    put: (record: DownloadRecord) => {
      records.set(record.id, record);
      return successRequest(record.id);
    },
  };

  const db = {
    objectStoreNames: {
      contains: (name: string) => created && name === "tracks",
    },
    createObjectStore: (name: string) => {
      expect(name).toBe("tracks");
      created = true;
      return store;
    },
    transaction: () => ({
      objectStore: (name: string) => {
        expect(name).toBe("tracks");
        return store;
      },
    }),
  };

  const indexedDb = {
    open: vi.fn(() => {
      const request = {
        result: db,
        error: null,
        onupgradeneeded: null,
        onsuccess: null,
        onerror: null,
      } as IDBOpenDBRequest & {
        result: typeof db;
        onupgradeneeded: ((this: IDBOpenDBRequest, ev: IDBVersionChangeEvent) => unknown) | null;
      };
      queueMicrotask(() => {
        request.onupgradeneeded?.call(request, new Event("upgradeneeded") as IDBVersionChangeEvent);
        queueMicrotask(() => request.onsuccess?.call(request, new Event("success")));
      });
      return request;
    }),
  };

  vi.stubGlobal("indexedDB", indexedDb);
  __downloadStoreForTests.resetDbPromise();

  return {
    records,
    open: indexedDb.open,
  };
}

function record(id: string, savedAt: number): DownloadRecord {
  return {
    id,
    savedAt,
    blob: new Blob([`audio-${id}`], { type: "audio/mpeg" }),
    coverBlob: new Blob([`cover-${id}`], { type: "image/jpeg" }),
    song: {
      id,
      title: `Track ${id}`,
      artist: "Artist",
      album: "Album",
      duration: 123,
      provider: "yandex",
      coverArt: `cover-${id}`,
    },
  };
}

describe("download store", () => {
  afterEach(() => {
    __downloadStoreForTests.resetDbPromise();
    vi.unstubAllGlobals();
  });

  it("writes and reads downloaded tracks with display metadata and cover blobs", async () => {
    installIndexedDbMock();
    const first = record("one", 10);

    await __downloadStoreForTests.putDownloadRecord(first);

    await expect(__downloadStoreForTests.getDownloadRecord("one")).resolves.toMatchObject({
      id: "one",
      savedAt: 10,
      song: {
        title: "Track one",
        artist: "Artist",
        album: "Album",
        provider: "yandex",
      },
    });
    expect((await __downloadStoreForTests.getDownloadRecord("one"))?.coverBlob?.size).toBeGreaterThan(0);
  });

  it("lists ids, lists records, and removes downloads", async () => {
    installIndexedDbMock();
    await __downloadStoreForTests.putDownloadRecord(record("one", 10));
    await __downloadStoreForTests.putDownloadRecord(record("two", 20));

    await expect(__downloadStoreForTests.loadDownloadedIds()).resolves.toEqual(["one", "two"]);
    await expect(__downloadStoreForTests.getDownloadRecords()).resolves.toHaveLength(2);

    await __downloadStoreForTests.deleteDownload("one");

    await expect(__downloadStoreForTests.loadDownloadedIds()).resolves.toEqual(["two"]);
  });
});
