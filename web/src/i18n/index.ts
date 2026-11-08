import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import "./types";

export const DEFAULT_LANGUAGE = "en";

// Keep <html lang> in step, so screen readers and the browser's hyphenation
// and spell checking use the right language.
i18n.on("languageChanged", (lng) => {
  if (typeof document !== "undefined") document.documentElement.lang = lng;
});

// The app's strings live in locales/ (one file per language, same key tree).
// A language that isn't fully translated falls back to English, key by key.
void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    fallbackLng: DEFAULT_LANGUAGE,
    supportedLngs: [DEFAULT_LANGUAGE],
    resources: { en: { translation: en } },
    // React escapes its output itself, so i18next must not HTML-escape values.
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
      lookupLocalStorage: "kinora.lang",
    },
  });

export { i18n };
