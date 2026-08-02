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

/**
 * [GRAIN] Resolve a locale tag (`zh-Hant-HK`, `de-AT`, a raw system locale) onto
 * a catalogue we ship.
 *
 * The RULE lives in Rust — `grain_locale::resolve` — not here. It used to be
 * implemented twice: once in this file and once in `tray_i18n.rs` for the tray
 * menu, which is how upstream came to fix only the TypeScript copy (`ea3c20a3`,
 * #1798). Which catalogue a system tag means is a fact about the machine, not a
 * decision about a screen, so it belongs on the backend — where a Rust test now
 * pins it against the tray's own copy so the two cannot disagree.
 */
export const resolveLocale = async (
  langCode: string | null | undefined,
): Promise<SupportedLanguageCode | null> => {
  if (!langCode) return null;
  try {
    return await commands.resolveAppLocale(langCode);
  } catch (e) {
    console.warn(`Failed to resolve locale "${langCode}":`, e);
    return null;
  }
};

/** Is this exact code one we ship? A membership test, NOT the resolution rule —
 *  callers holding an already-resolved code (a stored preference) need only
 *  this, and asking the backend for it would be a round-trip for nothing. */
export const isSupportedLanguage = (
  langCode: string | null | undefined,
): boolean =>
  Boolean(langCode) && SUPPORTED_LANGUAGES.some((l) => l.code === langCode);

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
      const supported = await resolveLocale(result.data.app_language);
      if (supported) {
        await setLanguage(supported);
      }
    } else {
      // Fall back to system locale detection if no saved preference
      const systemLocale = await locale();
      const supported = await resolveLocale(systemLocale);
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
