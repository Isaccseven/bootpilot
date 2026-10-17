import { createInstance } from "i18next";
import { initReactI18next } from "react-i18next";
import de from "./locales/de.json";
import en from "./locales/en.json";
import backendDe from "./locales/backend-de.json";
import backendEn from "./locales/backend-en.json";

export type Locale = "de" | "en";
export const LANGUAGE_STORAGE_KEY = "bootpilot.language.v1";

export function readStoredLocale(): Locale {
  try {
    return localStorage.getItem(LANGUAGE_STORAGE_KEY) === "en" ? "en" : "de";
  } catch {
    // Preferences are optional: denied storage must not prevent boot diagnosis.
    return "de";
  }
}

export const i18n = createInstance();
i18n.on("languageChanged", (language: string) => {
  if (typeof document !== "undefined")
    document.documentElement.lang = language === "en" ? "en" : "de";
});

// Everything ships with the app. No HTTP backend, external translation service,
// asynchronous language fetch or additional Tauri permission is necessary.
void i18n.use(initReactI18next).init({
  resources: {
    de: { translation: de, backend: backendDe },
    en: { translation: en, backend: backendEn },
  },
  lng: readStoredLocale(),
  fallbackLng: "de",
  supportedLngs: ["de", "en"],
  defaultNS: "translation",
  ns: ["translation", "backend"],
  initAsync: false,
  interpolation: { escapeValue: false }, // React escapes rendered strings.
  react: { useSuspense: false },
  returnNull: false,
});

export function changeLocale(value: string): void {
  const locale: Locale = value === "en" ? "en" : "de";
  void i18n.changeLanguage(locale);
  try {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, locale);
  } catch {
    // The active session can still switch language if persistence is denied.
  }
}

function formattingLocale(): string {
  return i18n.resolvedLanguage === "en" ? "en-GB" : "de-DE";
}

export function formatDate(value: string): string {
  const stamp = new Date(value);
  return Number.isFinite(stamp.getTime())
    ? new Intl.DateTimeFormat(formattingLocale(), {
        dateStyle: "medium",
        timeStyle: "medium",
      }).format(stamp)
    : i18n.t("common.unknownTime");
}

export function formatNumber(value: number): string {
  return new Intl.NumberFormat(formattingLocale(), {
    maximumFractionDigits: 1,
  }).format(value);
}
