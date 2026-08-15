/**
 * [GRAIN] ThemeContext — a view onto the backend's theme, not a store.
 *
 * This used to keep the preference in `localStorage` under two keys, one for
 * Settings and one for the Quick Panel. That could not be the source of truth:
 * Grain paints a native pill and a switcher capsule that have no browser
 * storage to read, and `extension-surface.ts` was reaching across to read the
 * settings window's key directly — which worked only because the two happen to
 * share an origin.
 *
 * The preference now lives in settings (`grain_theme` in Rust), which resolves
 * `system` against the OS and broadcasts the answer on both buses. This file
 * subscribes and re-renders. See `src-tauri/src/grain_theme.rs`.
 *
 * The two-independent-themes feature is gone. It existed for the Quick Panel,
 * which is being retired, and it is the thing that made a single source of
 * truth impossible.
 */
import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";
import { commands, events, type ThemeMode } from "@/bindings";

const LEGACY_KEYS = [
  "grain-theme-settings",
  "grain-theme-quick-panel",
  "grain-theme",
];

interface ThemeContextValue {
  /** What the user picked: follow the OS, or a fixed scheme. */
  mode: ThemeMode;
  /** What to paint. `mode` already resolved against the OS. */
  isDark: boolean;
  setMode: (mode: ThemeMode) => void;
  /** Flip between the two explicit modes. Leaves `system` deliberately: an
   *  explicit toggle is the user saying they want a specific scheme. */
  toggle: () => void;

  /** @deprecated Aliases kept so the pre-rewrite screens compile unchanged.
   *  Every surface reads one theme now; the Quick Panel names resolve to the
   *  same value as the Settings ones. */
  isSettingsDark: boolean;
  toggleSettings: () => void;
  isQuickPanelDark: boolean;
  toggleQuickPanel: () => void;
}

const noop = () => {};

const ThemeContext = createContext<ThemeContextValue>({
  mode: "system",
  isDark: false,
  setMode: noop,
  toggle: noop,
  isSettingsDark: false,
  toggleSettings: noop,
  isQuickPanelDark: false,
  toggleQuickPanel: noop,
});

/**
 * Carry a pre-backend preference across, once.
 *
 * The localStorage key is its own migration flag: it is read, forwarded and
 * deleted, so this is naturally idempotent and needs no "migrated" bookkeeping
 * in settings. A user who skips this release still has the key and still gets
 * migrated whenever they land on a build that has this code.
 */
const migrateLegacyPreference = async (): Promise<ThemeMode | null> => {
  try {
    const stored =
      localStorage.getItem("grain-theme-settings") ??
      localStorage.getItem("grain-theme");
    LEGACY_KEYS.forEach((key) => localStorage.removeItem(key));
    if (stored !== "dark" && stored !== "light") return null;
    const result = await commands.setThemeMode(stored);
    return result.status === "ok" ? result.data.mode : null;
  } catch {
    return null;
  }
};

export const ThemeProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [mode, setModeState] = useState<ThemeMode>("system");
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const apply = (next: { mode: ThemeMode; resolved: "light" | "dark" }) => {
      if (cancelled) return;
      setModeState(next.mode);
      setIsDark(next.resolved === "dark");
    };

    void (async () => {
      // The migration writes through the backend, so its answer is already the
      // resolved state — no second round trip when there was something to move.
      const migrated = await migrateLegacyPreference();
      if (cancelled) return;
      const state = await commands.getTheme();
      apply(state);
      if (migrated) setModeState(migrated);
    })();

    // The OS can flip under us while `mode` is "system"; the backend re-resolves
    // and tells every surface at once. Unsubscribe on unmount — a listener that
    // outlives its provider is a leak that survives every subsequent mount.
    const unlisten = events.themeChanged.listen((e) => apply(e.payload));

    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, []);

  const setMode = useCallback((next: ThemeMode) => {
    // Optimistic: the backend echoes back through `themeChanged`, but waiting
    // for the round trip would show a frame of the old scheme on the click.
    setModeState(next);
    void commands.setThemeMode(next).then((result) => {
      if (result.status === "ok") setIsDark(result.data.resolved === "dark");
    });
  }, []);

  const toggle = useCallback(
    () => setMode(isDark ? "light" : "dark"),
    [isDark, setMode],
  );

  return (
    <ThemeContext.Provider
      value={{
        mode,
        isDark,
        setMode,
        toggle,
        isSettingsDark: isDark,
        toggleSettings: toggle,
        isQuickPanelDark: isDark,
        toggleQuickPanel: toggle,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => useContext(ThemeContext);
