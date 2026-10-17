import { describe, expect, it, vi } from "vitest";
import { changeLocale, formatDate, formatNumber, i18n, LANGUAGE_STORAGE_KEY, readStoredLocale } from "./i18n";
import de from "./locales/de.json";
import en from "./locales/en.json";
import backendDe from "./locales/backend-de.json";
import backendEn from "./locales/backend-en.json";

it("uses German for missing or unsupported saved preferences and restores English", () => {
  expect(readStoredLocale()).toBe("de");
  localStorage.setItem(LANGUAGE_STORAGE_KEY, "fr");
  expect(readStoredLocale()).toBe("de");
  localStorage.setItem(LANGUAGE_STORAGE_KEY, "en");
  expect(readStoredLocale()).toBe("en");
  changeLocale(readStoredLocale());
  expect(document.documentElement.lang).toBe("en");
  changeLocale("invalid");
  expect(i18n.resolvedLanguage).toBe("de");
});

it("still switches language when storage is inaccessible", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new DOMException("Denied", "SecurityError"); });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new DOMException("Denied", "QuotaExceededError"); });
  expect(readStoredLocale()).toBe("de");
  expect(() => changeLocale("en")).not.toThrow();
  expect(document.documentElement.lang).toBe("en");
  expect(i18n.resolvedLanguage).toBe("en");
});

it("formats displayed dates and decimal values using the selected language", () => {
  const value = "2026-10-17T12:00:00Z";
  const options = { dateStyle: "medium", timeStyle: "medium" } as const;
  expect(formatDate(value)).toBe(new Intl.DateTimeFormat("de-DE", options).format(new Date(value)));
  expect(formatNumber(1234.5)).toBe("1.234,5");
  expect(formatDate("invalid")).toBe("Zeitpunkt unbekannt");
  changeLocale("en");
  expect(formatDate(value)).toBe(new Intl.DateTimeFormat("en-GB", options).format(new Date(value)));
  expect(formatNumber(1234.5)).toBe("1,234.5");
  expect(formatDate("invalid")).toBe("Unknown time");
});

function entries(value: Record<string, unknown>, prefix = ""): Record<string, string> {
  return Object.fromEntries(Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof child === "string" ? [[path, child]] : Object.entries(entries(child as Record<string, unknown>, path));
  }));
}

describe.each([["interface", de, en], ["native messages", backendDe, backendEn]] as const)("%s translations", (_name, german, english) => {
  it("provides every supported message and its interpolation values in both languages", () => {
    const source = entries(german);
    const target = entries(english);
    expect(Object.keys(target).sort()).toEqual(Object.keys(source).sort());
    for (const [key, message] of Object.entries(source)) {
      expect(target[key].trim(), key).not.toBe("");
      const placeholders = (text: string) => [...text.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g)].map((match) => match[1]).sort();
      expect(placeholders(target[key]), key).toEqual(placeholders(message));
    }
  });
});
