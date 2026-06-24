/**
 * [GRAIN] ThemeContext — single source of truth for the light/dark mode toggle.
 *
 * Both the Quick Panel and the Settings panel read and write through this
 * context so they always agree. The value is persisted to localStorage under
 * the key "grain-theme" so the preference survives app restarts.
 *
 * Usage:
 *   const { isDark, toggle } = useTheme();
 */
import React, { createContext, useCallback, useContext, useState } from "react";

const STORAGE_KEY = "grain-theme";

interface ThemeContextValue {
  isDark: boolean;
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  isDark: false,
  toggle: () => {},
});

export const ThemeProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [isDark, setIsDark] = useState<boolean>(() => {
    try {
      return localStorage.getItem(STORAGE_KEY) === "dark";
    } catch {
      return false;
    }
  });

  const toggle = useCallback(() => {
    setIsDark((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(STORAGE_KEY, next ? "dark" : "light");
      } catch {
        // storage unavailable — in-memory only
      }
      return next;
    });
  }, []);

  return (
    <ThemeContext.Provider value={{ isDark, toggle }}>
      {children}
    </ThemeContext.Provider>
  );
};

// eslint-disable-next-line react-refresh/only-export-components
export const useTheme = () => useContext(ThemeContext);
