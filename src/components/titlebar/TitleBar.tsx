import React, { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform } from "@tauri-apps/plugin-os";

/**
 * [GRAIN] Custom themed title bar.
 *
 * Drops the native OS chrome on Windows/Linux (the window is built frameless in
 * `build_main_window`) and owns the top strip instead — a drag region holding
 * the mono wordmark on the left, three window controls on the right.
 *
 * Aesthetic intent: Apple-calm (generous space, hairline border) leaning toward
 * Teenage Engineering (monospace wordmark + index mark). The close glyph turns
 * the warm status-error on hover so the destructive intent reads instantly
 * without a garish red button.
 *
 * On macOS the native overlay traffic-light buttons are kept (the window still
 * has decorations there), so this bar renders drag-only there — no controls.
 */
// [GRAIN] The top strip is now chrome-only: a drag region + window controls. The
// wordmark and the Quick Panel switch were moved off it (Quick Panel now lives at
// the bottom of the sidebar), so nothing renders on the left.
const TitleBar: React.FC = () => {
  const os = (() => {
    try {
      return platform();
    } catch {
      return "windows";
    }
  })();
  const isMac = os === "macos";

  const [maximized, setMaximized] = useState(false);
  const current = getCurrentWindow();

  // Track maximize state so the toggle glyph swaps between maximize/restore.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    current
      .isMaximized()
      .then(setMaximized)
      .catch(() => {});
    current
      .onResized(() => {
        current.isMaximized().then(setMaximized).catch(() => {});
      })
      .then((u) => (unlisten = u))
      .catch(() => {});
    return () => unlisten?.();
  }, [current]);

  const handleMinimize = () => current.minimize().catch(() => {});
  const handleToggleMaximize = () =>
    current.toggleMaximize().catch(() => {});
  // Close is intercepted by `on_window_event(CloseRequested)` in lib.rs, which
  // destroys the window (freeing WebView2 RAM) and lets the app live on in the
  // tray — so this is just a normal close request, not a force-quit.
  const handleClose = () => current.close().catch(() => {});

  // The whole bar drags the window; the buttons stop propagation so clicks
  // don't initiate a drag.
  const startDrag = (e: React.MouseEvent) => {
    // Only the empty regions drag — not interactions over the wordmark/controls.
    if ((e.target as HTMLElement).closest("[data-no-drag]")) return;
    current.startDragging().catch(() => {});
  };

  return (
    <div
      data-tauri-drag-region
      onMouseDown={startDrag}
      className="relative flex items-center justify-end h-9 shrink-0 border-b border-line bg-paper select-none"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      {/* Right: window controls (Windows/Linux only — macOS keeps native ones). */}
      {!isMac && (
        <div className="relative flex items-stretch h-full mr-2">
          <WindowButton label="Minimize" onClick={handleMinimize}>
            {/* minimize: a centered dash */}
            <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
              <path d="M1 5.5H10" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
            </svg>
          </WindowButton>

          <WindowButton
            label={maximized ? "Restore" : "Maximize"}
            onClick={handleToggleMaximize}
          >
            {maximized ? (
              /* restore: two offset rectangles (the classic TE/Win restore mark) */
              <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
                <rect x="2.5" y="0.5" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.3" fill="none" />
                <rect x="0.5" y="2.5" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.3" fill="var(--color-paper)" />
              </svg>
            ) : (
              /* maximize: a square */
              <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
                <rect x="1" y="1" width="9" height="9" rx="1" stroke="currentColor" strokeWidth="1.3" fill="none" />
              </svg>
            )}
          </WindowButton>

          <WindowButton label="Close" onClick={handleClose} variant="danger">
            {/* close: an X */}
            <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
              <path d="M1.5 1.5L9.5 9.5M9.5 1.5L1.5 9.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </WindowButton>
        </div>
      )}
    </div>
  );
};

interface WindowButtonProps {
  label: string;
  onClick: () => void;
  variant?: "default" | "danger";
  children: React.ReactNode;
}

const WindowButton: React.FC<WindowButtonProps> = ({
  label,
  onClick,
  variant = "default",
  children,
}) => {
  const isDanger = variant === "danger";
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onDoubleClick={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      // data-no-drag keeps startDragging from firing on control clicks.
      data-no-drag
      className={`grid place-items-center w-12 h-full text-ink-soft transition-colors duration-100 ${
        isDanger
          ? "hover:bg-[var(--status-error)] hover:text-[var(--on-accent)]"
          : "hover:bg-paper-sunken hover:text-ink"
      }`}
      style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
    >
      {children}
    </button>
  );
};

export default TitleBar;
