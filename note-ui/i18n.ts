import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "../src/i18n/locales/en/translation.json";

/**
 * [GRAIN] The note window's translations.
 *
 * English only, deliberately. The app's own i18n auto-discovers all 24 locales
 * and initialises from `@tauri-apps/plugin-os` — neither of which is available
 * here, and bundling two dozen translation files into a single inlined HTML
 * document would multiply the pack's size for strings most installs never read.
 *
 * When this needs more languages, they should be fetched rather than inlined.
 */
void i18n.use(initReactI18next).init({
  resources: { en: { translation: en } },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
