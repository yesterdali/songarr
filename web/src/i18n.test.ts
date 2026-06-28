import { describe, expect, it } from "vitest";

import {
  detectLanguage,
  hasMessage,
  loadLanguage,
  messageKeys,
  saveLanguage,
  translate,
  type Language,
} from "./i18n";

class MemoryStorage implements Storage {
  private values = new Map<string, string>();

  get length(): number {
    return this.values.size;
  }

  clear(): void {
    this.values.clear();
  }

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  key(index: number): string | null {
    return [...this.values.keys()][index] ?? null;
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

describe("i18n", () => {
  it("detects supported browser languages and falls back to English", () => {
    expect(detectLanguage("de-DE")).toBe("de");
    expect(detectLanguage(["fr-FR", "ru-RU"])).toBe("ru");
    expect(detectLanguage("pl-PL")).toBe("en");
  });

  it("persists a user language override", () => {
    const storage = new MemoryStorage();

    saveLanguage("ru", storage);

    expect(loadLanguage(storage)).toBe("ru");
  });

  it("ignores invalid stored languages", () => {
    const storage = new MemoryStorage();
    storage.setItem("songarr.language", "it");

    expect(loadLanguage(storage)).toBe("en");
  });

  it("formats translated messages with variables", () => {
    expect(translate("en", "tracksCount", { count: 3 })).toBe("3 tracks");
    expect(translate("de", "tracksCount", { count: 3 })).toBe("3 Titel");
    expect(translate("ru", "reasonSimilar", { seed: "Янка" })).toBe("Похоже на Янка");
  });

  it("keeps representative keys covered for every language", () => {
    const languages: Language[] = ["en", "de", "ru"];

    for (const language of languages) {
      for (const key of messageKeys()) {
        expect(hasMessage(language, key), `${language}.${key}`).toBe(true);
      }
    }
  });
});
