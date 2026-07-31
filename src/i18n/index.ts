import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { locale } from "@tauri-apps/plugin-os";
import { LANGUAGE_METADATA } from "./languages";
import en from "./locales/en/translation.json";
import { commands } from "@/bindings";
import {
  getLanguageDirection,
  updateDocumentDirection,
  updateDocumentLanguage,
} from "@/lib/utils/rtl";

// [GRAIN] Locales load ONE AT A TIME.
//
// This glob is deliberately NOT `{ eager: true }`. Eager gave every window all
// 24 catalogues — ~751 KB of JSON parsed at startup to render in one language —
// and because main.tsx imports this module before it branches on the window
// label, the Agent panel paid for it too, despite being the one surface built
// to stay small (it skips UI scaling and the model store for exactly that
// reason). Lazy, the glob still enumerates which locales exist without reading
// any of them, so the language list below costs nothing.
// English is excluded from the glob and imported statically below: it is the
// fallback, so it must be in the main chunk either way, and listing it here too
// would leave rollup warning that one module is both statically and dynamically
// imported.
const localeModules = import.meta.glob<{ default: Record<string, unknown> }>(
  ["./locales/*/translation.json", "!./locales/en/translation.json"],
);

const codeOf = (path: string) =>
  path.match(/\.\/locales\/(.+)\/translation\.json/)?.[1];

const AVAILABLE: string[] = [
  "en",
  ...Object.keys(localeModules)
    .map(codeOf)
    .filter((c): c is string => Boolean(c)),
];

const loaderFor = (code: string) =>
  Object.entries(localeModules).find(([p]) => codeOf(p) === code)?.[1];

/** Fetch a catalogue and hand it to i18next, once. English is already bundled,
 *  and i18next keeps what it has been given, so repeat calls are free. */
export const loadLocale = async (code: string): Promise<boolean> => {
  if (code === "en" || i18n.hasResourceBundle(code, "translation")) return true;
  const load = loaderFor(code);
  if (!load) return false;
  try {
    const mod = await load();
    i18n.addResourceBundle(code, "translation", mod.default, true, true);
    return true;
  } catch (e) {
    // A missing catalogue is not fatal: fallbackLng renders English.
    console.warn(`Failed to load locale "${code}":`, e);
    return false;
  }
};

/** Load then switch — the only correct order. Switching first would paint one
 *  frame of English before the catalogue arrives. */
export const setLanguage = async (code: string): Promise<void> => {
  if (code === i18n.language) return;
  await loadLocale(code);
  await i18n.changeLanguage(code);
};

// Build supported languages list from discovered locales + metadata
export const SUPPORTED_LANGUAGES = AVAILABLE
  .map((code) => {
    const meta = LANGUAGE_METADATA[code];
    if (!meta) {
      console.warn(`Missing metadata for locale "${code}" in languages.ts`);
      return { code, name: code, nativeName: code, priority: undefined };
    }
    return {
      code,
      name: meta.name,
      nativeName: meta.nativeName,
      priority: meta.priority,
    };
  })
  .sort((a, b) => {
    // Sort by priority first (lower = higher), then alphabetically
    if (a.priority !== undefined && b.priority !== undefined) {
      return a.priority - b.priority;
    }
    if (a.priority !== undefined) return -1;
    if (b.priority !== undefined) return 1;
    return a.name.localeCompare(b.name);
  });

export type SupportedLanguageCode = string;

// Check if a language code is supported
export const getSupportedLanguage = (
  langCode: string | null | undefined,
): SupportedLanguageCode | null => {
  if (!langCode) return null;

  const normalized = langCode.toLowerCase().replace(/_/g, "-");
  const subtags = normalized.split("-");
  const language = subtags[0];
  const isHant = subtags.includes("hant");
  const isHans = subtags.includes("hans");
  const isTraditionalRegion = ["tw", "hk", "mo"].some((region) =>
    subtags.includes(region),
  );

  // Try exact match first
  let supported = SUPPORTED_LANGUAGES.find(
    (lang) => lang.code.toLowerCase() === normalized,
  );
  if (!supported) {
    let fallback = language;
    if (language === "zh" && (isHant || (!isHans && isTraditionalRegion))) {
      fallback = "zh-tw";
    } else if (language === "yue") {
      // Cantonese uses Traditional Chinese unless explicitly tagged as Hans.
      fallback = isHans ? "zh" : "zh-tw";
    }
    supported = SUPPORTED_LANGUAGES.find(
      (lang) => lang.code.toLowerCase() === fallback,
    );
  }
  return supported ? supported.code : null;
};

// Initialize i18n with English as default
// Language will be synced from settings after init
i18n.use(initReactI18next).init({
  // English only at boot — it is both the initial language and the fallback,
  // so it is the one catalogue that must be present synchronously for the
  // first paint. Everything else arrives through `loadLocale`.
  resources: { en: { translation: en } },
  lng: "en",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false, // React already escapes values
  },
  react: {
    useSuspense: false, // Disable suspense for SSR compatibility
  },
});

// Sync language from app settings
export const syncLanguageFromSettings = async () => {
  try {
    const result = await commands.getAppSettings();
    if (result.status === "ok" && result.data.app_language) {
      const supported = getSupportedLanguage(result.data.app_language);
      if (supported) {
        await setLanguage(supported);
      }
    } else {
      // Fall back to system locale detection if no saved preference
      const systemLocale = await locale();
      const supported = getSupportedLanguage(systemLocale);
      if (supported) {
        await setLanguage(supported);
      }
    }
  } catch (e) {
    console.warn("Failed to sync language from settings:", e);
  }
};

// Run language sync on init
syncLanguageFromSettings();

// Listen for language changes to update HTML dir and lang attributes
i18n.on("languageChanged", (lng) => {
  const dir = getLanguageDirection(lng);
  updateDocumentDirection(dir);
  updateDocumentLanguage(lng);
});

// Re-export RTL utilities for convenience
export { getLanguageDirection, isRTLLanguage } from "@/lib/utils/rtl";

export default i18n;
