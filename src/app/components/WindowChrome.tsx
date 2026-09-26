import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform } from "@tauri-apps/plugin-os";
import { Copy, Square, Minus, Moon, Sun, X } from "lucide-react";
import { useTheme } from "@/contexts/ThemeContext";

/** Native controls shared by the workspace and every first-run screen. */
export function WindowChrome() {
  const { isDark, setMode } = useTheme();
  const currentWindow = useMemo(() => getCurrentWindow(), []);
  const [maximized, setMaximized] = useState(false);
  const isMac = useMemo(() => platform() === "macos", []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      void currentWindow
        .isMaximized()
        .then((value) => {
          if (!disposed) setMaximized(value);
        })
        .catch(() => {});
    };
    refresh();
    void currentWindow
      .onResized(refresh)
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [currentWindow]);

  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onMouseDown={(event) => {
        if (
          event.button !== 0 ||
          (event.target as HTMLElement).closest("[data-no-drag]")
        )
          return;
        if (event.detail === 2)
          void currentWindow.toggleMaximize().catch(() => {});
        else void currentWindow.startDragging().catch(() => {});
      }}
      style={{ WebkitAppRegion: "drag" } as CSSProperties}
    >
      <div
        className="window-actions"
        data-no-drag
        style={{ WebkitAppRegion: "no-drag" } as CSSProperties}
      >
        <button
          className="theme-toggle"
          type="button"
          aria-label={`Switch to ${isDark ? "light" : "dark"} mode`}
          title={`Switch to ${isDark ? "light" : "dark"} mode`}
          onClick={() => setMode(isDark ? "light" : "dark")}
        >
          <Sun className="icon" aria-hidden="true" />
          <Moon className="icon moon-icon" aria-hidden="true" />
        </button>
        {!isMac && (
          <>
            <button
              className="window-button"
              type="button"
              aria-label="Minimize"
              title="Minimize"
              onClick={() => void currentWindow.minimize().catch(() => {})}
            >
              <Minus className="icon sm" aria-hidden="true" />
            </button>
            <button
              className="window-button"
              type="button"
              aria-label={maximized ? "Restore" : "Maximize"}
              title={maximized ? "Restore" : "Maximize"}
              onClick={() =>
                void currentWindow.toggleMaximize().catch(() => {})
              }
            >
              {maximized ? (
                <Copy className="icon sm" aria-hidden="true" />
              ) : (
                <Square className="icon sm" aria-hidden="true" />
              )}
            </button>
            <button
              className="window-button close"
              type="button"
              aria-label="Close"
              title="Close"
              onClick={() => void currentWindow.close().catch(() => {})}
            >
              <X className="icon sm" aria-hidden="true" />
            </button>
          </>
        )}
      </div>
    </header>
  );
}
