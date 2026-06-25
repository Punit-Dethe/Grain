/**
 * [GRAIN] ThemeContext — Independent toggles for Settings and Quick Panel.
 *
 * Exposes two independent states so users can have a dark Quick Panel and a light
 * Settings window, or vice versa.
 */
import React, { createContext, useCallback, useContext, useState } from "react";

const SETTINGS_KEY = "grain-theme-settings";
const QP_KEY = "grain-theme-quick-panel";
const LEGACY_KEY = "grain-theme";

interface ThemeContextValue {
  isSettingsDark: boolean;
  toggleSettings: () => void;
  isQuickPanelDark: boolean;
  toggleQuickPanel: () => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  isSettingsDark: false,
  toggleSettings: () => {},
  isQuickPanelDark: false,
  toggleQuickPanel: () => {},
});

export const ThemeProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  // Migration logic: If the old single-theme key exists, read it, then delete it.
  // We use this value to seed both new keys so the user's preference isn't lost.
  const [isSettingsDark, setIsSettingsDark] = useState<boolean>(() => {
    try {
      const legacy = localStorage.getItem(LEGACY_KEY);
      if (legacy !== null) {
        localStorage.setItem(SETTINGS_KEY, legacy);
        localStorage.setItem(QP_KEY, legacy);
        localStorage.removeItem(LEGACY_KEY);
        return legacy === "dark";
      }
      return localStorage.getItem(SETTINGS_KEY) === "dark";
    } catch {
      return false;
    }
  });

  const [isQuickPanelDark, setIsQuickPanelDark] = useState<boolean>(() => {
    try {
      return localStorage.getItem(QP_KEY) === "dark";
    } catch {
      return false;
    }
  });

  const toggleSettings = useCallback(() => {
    setIsSettingsDark((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(SETTINGS_KEY, next ? "dark" : "light");
      } catch {
        // storage unavailable
      }
      return next;
    });
  }, []);

  const toggleQuickPanel = useCallback(() => {
    setIsQuickPanelDark((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(QP_KEY, next ? "dark" : "light");
      } catch {
        // storage unavailable
      }
      return next;
    });
  }, []);

  return (
    <ThemeContext.Provider
      value={{
        isSettingsDark,
        toggleSettings,
        isQuickPanelDark,
        toggleQuickPanel,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
};

// eslint-disable-next-line react-refresh/only-export-components
export const useTheme = () => useContext(ThemeContext);
