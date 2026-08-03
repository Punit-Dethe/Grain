import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform } from "@tauri-apps/plugin-os";
import { Toaster } from "sonner";
import { HistorySettings } from "@/components/settings/history/HistorySettings";
import { AudioPlayerGroup } from "@/components/ui/AudioPlayer";
import Onboarding, { AccessibilityOnboarding } from "@/components/onboarding";
import { commands, type OnboardingStep } from "@/bindings";
import { ThemeProvider, useTheme } from "@/contexts/ThemeContext";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import {
  routeFromHash,
  routeUsesCompactGlobalRail,
  type AppRoute,
} from "./navigation";
import { SettingsPage } from "./pages/SettingsPage";
import { NotesPage } from "./pages/NotesPage";
import { ToolsPage } from "./pages/ToolsPage";
import { ExtensionsPage, ExtensionSettingsPage } from "./pages/ExtensionsPage";
import { HistoryCard, type HistoryViewMode } from "./history/HistoryCard";
import {
  hasProcessedText,
  useHistoryController,
  type HistoryController,
} from "./history/useHistoryController";
import { OverviewCards } from "./overview/OverviewCards";
import { UpdateNotice } from "@/components/UpdateNotice";
import "./app.css";

let onboardingResolution: ReturnType<
  typeof commands.resolveOnboardingState
> | null = null;

const PROTOTYPE_COPY = {
  quickPanel: "Quick panel",
  quickPanelShortcut: "Ctrl K",
  original: "Original",
  processed: "AI processed",
  heroTopline: "On-device · Parakeet TDT 0.6B",
  heroKicker: "Grain is listening when you are",
  heroTitle: "Speak before the thought disappears.",
  heroBody:
    "Grain captures the words, preserves the intent, and places the result exactly where your work continues.",
  startFlow: "Start Flow",
  openNotes: "Open notes",
  quickActions: "Start here",
  quickActionsBody:
    "Your keys, your words, and what Grain can be taught to do.",
  recent: "Recent transcriptions",
  recentBody: "Text first. Audio remains available when you need to verify it.",
  viewAll: "View all",
  recentLoading: "Loading recent transcriptions…",
  recentError: "Recent transcriptions could not be loaded.",
  recentEmpty: "No transcriptions yet.",
  recentNoProcessed: "No AI-processed transcriptions yet.",
  retry: "Retry",
} as const;

function resolveOnboardingState() {
  if (onboardingResolution) return onboardingResolution;

  onboardingResolution = commands.resolveOnboardingState().then(
    (result) => {
      onboardingResolution = null;
      return result;
    },
    (error) => {
      onboardingResolution = null;
      throw error;
    },
  );
  return onboardingResolution;
}

function useHashRoute(): AppRoute {
  const [route, setRoute] = useState<AppRoute>(() =>
    routeFromHash(window.location.hash),
  );

  useEffect(() => {
    const onHashChange = () => setRoute(routeFromHash(window.location.hash));
    window.addEventListener("hashchange", onHashChange);
    if (!window.location.hash) {
      window.history.replaceState(null, "", "#/overview");
    }
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  return route;
}

type IconName =
  | "home"
  | "note"
  | "clock"
  | "sliders"
  | "box"
  | "zap"
  | "command"
  | "sun"
  | "moon"
  | "min"
  | "max"
  | "copy"
  | "close"
  | "panel"
  | "folder"
  | "search"
  | "play"
  | "pause"
  | "star"
  | "refresh"
  | "trash";

function Icon({ name, small = false }: { name: IconName; small?: boolean }) {
  return (
    <svg className={`icon${small ? " sm" : ""}`} aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

function IconSprite() {
  return (
    <svg className="prototype-icon-sprite" aria-hidden="true">
      <symbol id="i-home" viewBox="0 0 24 24">
        <path d="M3 10.5 12 3l9 7.5" />
        <path d="M5.5 9.5V21h13V9.5" />
        <path d="M9.5 21v-6h5v6" />
      </symbol>
      <symbol id="i-note" viewBox="0 0 24 24">
        <path d="M6 3.5h9l3 3V20.5H6z" />
        <path d="M15 3.5v4h4" />
        <path d="M9 12h6M9 16h5" />
      </symbol>
      <symbol id="i-sliders" viewBox="0 0 24 24">
        <path d="M4 7h10M18 7h2M4 17h3M11 17h9M14 4v6M8 14v6" />
      </symbol>
      <symbol id="i-box" viewBox="0 0 24 24">
        <path d="m12 3 8 4.5v9L12 21l-8-4.5v-9z" />
        <path d="m4.5 7.8 7.5 4.3 7.5-4.3M12 12v9" />
      </symbol>
      <symbol id="i-zap" viewBox="0 0 24 24">
        <path d="m13 2-9 12h8l-1 8 9-12h-8z" />
      </symbol>
      <symbol id="i-clock" viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="9" />
        <path d="M12 7v5l3.5 2" />
      </symbol>
      <symbol id="i-command" viewBox="0 0 24 24">
        <path d="M9 6a3 3 0 1 0-3 3h3zM15 6a3 3 0 1 1 3 3h-3zM9 15H6a3 3 0 1 0 3 3zM15 15h3a3 3 0 1 1-3 3zM9 9h6v6H9z" />
      </symbol>
      <symbol id="i-panel" viewBox="0 0 24 24">
        <rect height="16" rx="2" width="18" x="3" y="4" />
        <path d="M9 4v16" />
      </symbol>
      <symbol id="i-close" viewBox="0 0 24 24">
        <path d="m6 6 12 12M18 6 6 18" />
      </symbol>
      <symbol id="i-folder" viewBox="0 0 24 24">
        <path d="M3 6h7l2 2h9v11H3z" />
      </symbol>
      <symbol id="i-search" viewBox="0 0 24 24">
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-4-4" />
      </symbol>
      <symbol id="i-min" viewBox="0 0 24 24">
        <path d="M6 12h12" />
      </symbol>
      <symbol id="i-max" viewBox="0 0 24 24">
        <rect height="10" rx="1" width="10" x="7" y="7" />
      </symbol>
      <symbol id="i-copy" viewBox="0 0 24 24">
        <rect height="11" rx="2" width="11" x="8" y="8" />
        <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
      </symbol>
      <symbol id="i-star" viewBox="0 0 24 24">
        <path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-2.9-5.6 2.9 1.1-6.2L3 9.6l6.2-.9z" />
      </symbol>
      <symbol id="i-refresh" viewBox="0 0 24 24">
        <path d="M20 6v5h-5M4 18v-5h5" />
        <path d="M18.5 9A7 7 0 0 0 6.2 6.2L4 8M5.5 15A7 7 0 0 0 17.8 17.8L20 16" />
      </symbol>
      <symbol id="i-trash" viewBox="0 0 24 24">
        <path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" />
      </symbol>
      <symbol id="i-play" viewBox="0 0 24 24">
        <path d="m8 5 11 7-11 7z" />
      </symbol>
      <symbol id="i-pause" viewBox="0 0 24 24">
        <path d="M9 5v14M15 5v14" />
      </symbol>
      <symbol id="i-sun" viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="3.5" />
        <path d="M12 2.5v2M12 19.5v2M4.6 4.6 6 6M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4" />
      </symbol>
      <symbol id="i-moon" viewBox="0 0 24 24">
        <path d="M20 15.2A8.4 8.4 0 0 1 8.8 4 8.4 8.4 0 1 0 20 15.2Z" />
      </symbol>
    </svg>
  );
}

function WindowChrome({ route }: { route: AppRoute }) {
  const { isDark, setMode } = useTheme();
  const currentWindow = useMemo(() => getCurrentWindow(), []);
  const [maximized, setMaximized] = useState(false);
  const isMac = useMemo(() => {
    try {
      return platform() === "macos";
    } catch {
      return false;
    }
  }, []);

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

  const startDrag = (event: React.MouseEvent<HTMLElement>) => {
    if ((event.target as HTMLElement).closest("[data-no-drag]")) return;
    void currentWindow.startDragging().catch(() => {});
  };

  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onMouseDown={startDrag}
      style={{ WebkitAppRegion: "drag" } as CSSProperties}
    >
      <div className="workspace-title">
        {route.page === "settings"
          ? "Settings"
          : route.page === "notes"
            ? "Notes"
            : route.page === "history"
              ? "History"
              : route.page === "tools"
                ? "Studio"
                : route.page === "extensions"
                  ? "Extensions"
                  : route.page === "extension-settings"
                    ? "Extension settings"
                    : "Overview"}
      </div>
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
          <Icon name="sun" />
          <svg className="icon moon-icon" aria-hidden="true">
            <use href="#i-moon" />
          </svg>
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
              <Icon name="min" small />
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
              <Icon name={maximized ? "copy" : "max"} small />
            </button>
            <button
              className="window-button close"
              type="button"
              aria-label="Close"
              title="Close"
              onClick={() => void currentWindow.close().catch(() => {})}
            >
              <Icon name="close" small />
            </button>
          </>
        )}
      </div>
    </header>
  );
}

const NAV_GROUPS = [
  {
    label: "Workspace",
    items: [
      { page: "overview", label: "Overview", icon: "home", href: "#/overview" },
      { page: "notes", label: "Notes", icon: "note", href: "#/notes" },
      { page: "history", label: "History", icon: "clock", href: "#/history" },
      {
        page: "tools",
        label: "Studio",
        icon: "zap",
        href: "#/tools/dictionary",
      },
    ],
  },
  {
    label: "Configure",
    items: [
      {
        page: "settings",
        label: "Settings",
        icon: "sliders",
        href: "#/settings/capture",
      },
      {
        page: "extensions",
        label: "Extensions",
        icon: "box",
        href: "#/extensions/installed",
      },
    ],
  },
] as const;

function Sidebar({ route }: { route: AppRoute }) {
  const currentModel = useModelStore((state) => state.currentModel);
  const models = useModelStore((state) => state.models);
  const loading = useModelStore((state) => state.loading);
  const modelName = useMemo(
    () =>
      models.find((model) => model.id === currentModel)?.name ??
      currentModel ??
      "Parakeet TDT 0.6B",
    [currentModel, models],
  );

  return (
    <aside aria-label="Primary navigation" className="sidebar">
      {/* The sidebar now runs the full height of the window, so the wordmark
          lives at its head rather than in the titlebar. This strip stays a
          drag region so the top-left corner still moves the window. */}
      <div
        className="sidebar-brand"
        data-tauri-drag-region
        style={{ WebkitAppRegion: "drag" } as CSSProperties}
      >
        <div className="grain-wordmark">
          <strong>GRAIN</strong>
        </div>
      </div>
      {NAV_GROUPS.map((group) => (
        <nav className="nav-section" key={group.label}>
          <div className="nav-label">{group.label}</div>
          <div className="nav-list">
            {group.items.map((item) => {
              const active =
                item.page === route.page ||
                (item.page === "extensions" &&
                  route.page === "extension-settings");
              const href = "href" in item ? item.href : undefined;
              return (
                <button
                  key={item.page}
                  type="button"
                  className={`nav-item${active ? " active" : ""}`}
                  data-page={item.page}
                  title={item.label}
                  disabled={!href}
                  aria-current={active ? "page" : undefined}
                  onClick={() => {
                    if (href) window.location.hash = href.slice(1);
                  }}
                >
                  <Icon name={item.icon} />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </div>
        </nav>
      ))}
      <div className="sidebar-spacer" />
      <UpdateNotice />
      <div className="model-status">
        <div className="status-row">
          <strong>{loading ? "Checking model" : modelName}</strong>
        </div>
        <p>{loading ? "Checking · on device" : "Loaded · on device"}</p>
      </div>
      <button
        className="quick-panel-button"
        type="button"
        disabled
        aria-label="Quick panel (not available in this phase)"
      >
        <Icon name="command" />
        <span>{PROTOTYPE_COPY.quickPanel}</span>
        <kbd>{PROTOTYPE_COPY.quickPanelShortcut}</kbd>
      </button>
    </aside>
  );
}

const DARK_PALETTE = [
  [9, 11, 15],
  [18, 23, 31],
  [29, 37, 51],
  [44, 57, 80],
  [63, 82, 118],
  [92, 118, 170],
  [120, 148, 216],
  [142, 168, 253],
  [176, 198, 255],
] as const;

const LIGHT_PALETTE = [
  [248, 249, 252],
  [236, 240, 247],
  [221, 228, 240],
  [201, 214, 233],
  [175, 194, 226],
  [144, 171, 219],
  [114, 144, 205],
  [96, 125, 193],
  [142, 168, 253],
] as const;

const BAYER_8 = [
  [0, 48, 12, 60, 3, 51, 15, 63],
  [32, 16, 44, 28, 35, 19, 47, 31],
  [8, 56, 4, 52, 11, 59, 7, 55],
  [40, 24, 36, 20, 43, 27, 39, 23],
  [2, 50, 14, 62, 1, 49, 13, 61],
  [34, 18, 46, 30, 33, 17, 45, 29],
  [10, 58, 6, 54, 9, 57, 5, 53],
  [42, 26, 38, 22, 41, 25, 37, 21],
] as const;

function DitherCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { isDark } = useTheme();

  useEffect(() => {
    const canvas = canvasRef.current;
    const hero = canvas?.closest<HTMLElement>(".hero");
    const context = canvas?.getContext("2d", {
      alpha: false,
      desynchronized: true,
    });
    if (!canvas || !hero || !context) return;

    context.imageSmoothingEnabled = false;
    const palette = isDark ? DARK_PALETTE : LIGHT_PALETTE;
    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    const pointer = {
      x: 0.72,
      y: 0.34,
      tx: 0.72,
      ty: 0.34,
      active: false,
    };
    let width = 300;
    let height = 120;
    let imageData = context.createImageData(width, height);
    let animationFrame = 0;
    let lastFrame = 0;

    const clamp = (value: number, min = 0, max = 1) =>
      Math.max(min, Math.min(max, value));
    const smoothstep = (a: number, b: number, value: number) => {
      const amount = clamp((value - a) / (b - a));
      return amount * amount * (3 - 2 * amount);
    };
    const mix = (a: number, b: number, amount: number) => a + (b - a) * amount;
    const orb = (
      x: number,
      y: number,
      centerX: number,
      centerY: number,
      radius: number,
      softness = 1.75,
    ) =>
      Math.pow(
        clamp(1 - Math.hypot(x - centerX, y - centerY) / radius),
        softness,
      );

    const resizeCanvas = () => {
      const heroWidth = hero.clientWidth || 1200;
      const heroHeight = hero.clientHeight || 420;
      width = Math.max(220, Math.min(360, Math.round(heroWidth / 5.2)));
      height = Math.max(84, Math.min(168, Math.round(heroHeight / 3.1)));
      canvas.width = width;
      canvas.height = height;
      imageData = context.createImageData(width, height);
    };

    const handlePointerMove = (event: PointerEvent) => {
      const rect = hero.getBoundingClientRect();
      pointer.tx = clamp((event.clientX - rect.left) / rect.width);
      pointer.ty = clamp((event.clientY - rect.top) / rect.height);
      pointer.active = true;
    };
    const handlePointerLeave = () => {
      pointer.tx = 0.72;
      pointer.ty = 0.34;
      pointer.active = false;
    };

    const field = (pixelX: number, pixelY: number, time: number) => {
      let x = pixelX / width;
      let y = pixelY / height;
      pointer.x = mix(pointer.x, pointer.tx, 0.08);
      pointer.y = mix(pointer.y, pointer.ty, 0.08);

      x +=
        Math.sin(y * 7.2 + time * 0.68) * 0.045 +
        Math.cos(x * 6.4 - time * 0.34) * 0.032;
      y +=
        Math.cos(x * 8.8 - time * 0.54) * 0.048 +
        Math.sin((x + y) * 5.2 + time * 0.24) * 0.024;

      const baseWaves =
        0.08 +
        0.09 * Math.sin(x * 7.8 + time * 0.58) +
        0.07 * Math.cos(y * 9.4 - time * 0.36) +
        0.05 * Math.sin((x + y) * 10.5 - time * 0.41);
      const orbitA = orb(
        x,
        y,
        0.18 + Math.sin(time * 0.27) * 0.08,
        0.29 + Math.cos(time * 0.19) * 0.08,
        0.31,
        1.8,
      );
      const orbitB = orb(
        x,
        y,
        0.81 + Math.cos(time * 0.21) * 0.06,
        0.37 + Math.sin(time * 0.23) * 0.07,
        0.29,
        1.85,
      );
      const orbitC = orb(
        x,
        y,
        0.53 + Math.sin(time * 0.15) * 0.06,
        0.79 + Math.cos(time * 0.18) * 0.05,
        0.34,
        1.95,
      );
      const orbitD = orb(
        x,
        y,
        pointer.x,
        pointer.y,
        pointer.active ? 0.24 : 0.19,
        2.1,
      );
      const topMist = orb(x, y, 0.5, -0.14, 0.9, 2.3) * 0.12;
      const edgeFade = smoothstep(
        1.05,
        0.38,
        Math.hypot((x - 0.5) * 1.12, (y - 0.5) * 1.02),
      );

      return clamp(
        (baseWaves +
          orbitA * 0.36 +
          orbitB * 0.3 +
          orbitC * 0.25 +
          orbitD * 0.18 +
          topMist) *
          mix(0.72, 1, edgeFade),
      );
    };

    const draw = (milliseconds: number) => {
      if (!reducedMotion && milliseconds - lastFrame < 1000 / 28) {
        animationFrame = requestAnimationFrame(draw);
        return;
      }
      lastFrame = milliseconds;
      const time = reducedMotion ? 0 : milliseconds / 1000;
      const data = imageData.data;
      const bands = palette.length - 1;

      for (let y = 0; y < height; y += 1) {
        for (let x = 0; x < width; x += 1) {
          const value = field(x, y, time);
          const scaled = value * bands;
          let index = Math.floor(scaled);
          const threshold = (BAYER_8[y & 7][x & 7] + 0.5) / 64;
          const flutter =
            ((Math.sin(x * 0.19 + y * 0.13 + time * 5.2) + 1) * 0.5 - 0.5) *
            0.045;
          if (scaled - index > threshold + flutter) index += 1;
          index = Math.max(0, Math.min(bands, index));
          const color = palette[index];
          const offset = (y * width + x) * 4;
          data[offset] = color[0];
          data[offset + 1] = color[1];
          data[offset + 2] = color[2];
          data[offset + 3] = 255;
        }
      }

      context.putImageData(imageData, 0, 0);
      if (!reducedMotion) animationFrame = requestAnimationFrame(draw);
    };

    resizeCanvas();
    const resizeObserver = new ResizeObserver(resizeCanvas);
    resizeObserver.observe(hero);
    window.addEventListener("resize", resizeCanvas, { passive: true });
    hero.addEventListener("pointermove", handlePointerMove);
    hero.addEventListener("pointerleave", handlePointerLeave);
    animationFrame = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
      window.removeEventListener("resize", resizeCanvas);
      hero.removeEventListener("pointermove", handlePointerMove);
      hero.removeEventListener("pointerleave", handlePointerLeave);
    };
  }, [isDark]);

  return <canvas ref={canvasRef} aria-hidden="true" />;
}

function ViewSwitch({
  mode,
  onChange,
  label,
}: {
  mode: "original" | "processed";
  onChange: (mode: "original" | "processed") => void;
  label: string;
}) {
  return (
    <div aria-label={label} className="view-switch">
      <button
        className={mode === "original" ? "active" : ""}
        type="button"
        onClick={() => onChange("original")}
      >
        {PROTOTYPE_COPY.original}
      </button>
      <button
        className={mode === "processed" ? "active" : ""}
        type="button"
        onClick={() => onChange("processed")}
      >
        {PROTOTYPE_COPY.processed}
      </button>
    </div>
  );
}

function OverviewPage({ history }: { history: HistoryController }) {
  const [mode, setMode] = useState<HistoryViewMode>("original");

  // AI processed view lists only entries the AI actually rewrote — raw
  // transcripts are hidden so the two are never confused. Original view shows
  // everything. Either way the recent strip is capped at three.
  const recentEntries = (
    mode === "processed"
      ? history.entries.filter(hasProcessedText)
      : history.entries
  ).slice(0, 3);

  return (
    <section className="page active" data-page-panel="overview">
      <div className="page-wrap wide">
        <div className="hero refined-hero">
          <button
            className="icon-button hero-design-trigger"
            type="button"
            disabled
            aria-label="Design system panel is not available in this phase"
          >
            <Icon name="panel" />
          </button>
          <DitherCanvas />
          <div className="hero-topline">
            <span className="live-dot" />
            {PROTOTYPE_COPY.heroTopline}
          </div>
          <div className="hero-content">
            <div className="hero-copy">
              <div className="hero-kicker">{PROTOTYPE_COPY.heroKicker}</div>
              <h2>{PROTOTYPE_COPY.heroTitle}</h2>
              <p>{PROTOTYPE_COPY.heroBody}</p>
            </div>
            <div className="hero-actions">
              <button className="button primary" type="button" disabled>
                {PROTOTYPE_COPY.startFlow}
              </button>
              <button
                className="button secondary-glass"
                type="button"
                onClick={() => {
                  window.location.hash = "/notes";
                }}
              >
                {PROTOTYPE_COPY.openNotes}
              </button>
            </div>
          </div>
        </div>

        <div className="section-head compact-section-head">
          <div>
            <h2>{PROTOTYPE_COPY.quickActions}</h2>
            <p>{PROTOTYPE_COPY.quickActionsBody}</p>
          </div>
        </div>
        <OverviewCards />

        <div className="section-head transcript-section-head">
          <div>
            <h2>{PROTOTYPE_COPY.recent}</h2>
            <p>{PROTOTYPE_COPY.recentBody}</p>
          </div>
          <div className="section-head-actions">
            <ViewSwitch
              mode={mode}
              onChange={setMode}
              label="Recent transcription view"
            />
            <button
              className="text-button"
              type="button"
              onClick={() => {
                window.location.hash = "/history";
              }}
            >
              {PROTOTYPE_COPY.viewAll}
            </button>
          </div>
        </div>
        <div className="transcript-feed recent-feed">
          {history.loading ? (
            <div className="history-state" role="status">
              {PROTOTYPE_COPY.recentLoading}
            </div>
          ) : history.loadError && history.entries.length === 0 ? (
            <div className="history-state history-state-error">
              <p>{PROTOTYPE_COPY.recentError}</p>
              <button
                className="button"
                type="button"
                onClick={() => void history.reload()}
              >
                {PROTOTYPE_COPY.retry}
              </button>
            </div>
          ) : history.entries.length === 0 ? (
            <div className="history-state">{PROTOTYPE_COPY.recentEmpty}</div>
          ) : recentEntries.length === 0 ? (
            <div className="history-state">
              {PROTOTYPE_COPY.recentNoProcessed}
            </div>
          ) : (
            <AudioPlayerGroup>
              {recentEntries.map((entry) => (
                <HistoryCard
                  key={entry.id}
                  entry={entry}
                  viewMode={mode}
                  controller={history}
                />
              ))}
            </AudioPlayerGroup>
          )}
        </div>
      </div>
    </section>
  );
}

function NextShell() {
  const route = useHashRoute();
  const history = useHistoryController();
  const { isDark } = useTheme();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  const [isReturningUser, setIsReturningUser] = useState(false);
  const { settings, updateSetting, refreshAudioDevices, refreshOutputDevices } =
    useSettings();
  const hasInitializedRuntime = useRef(false);

  useEffect(() => {
    document.documentElement.dataset.theme = isDark ? "dark" : "light";
    return () => {
      delete document.documentElement.dataset.theme;
    };
  }, [isDark]);

  useEffect(() => {
    let active = true;
    void resolveOnboardingState()
      .then((result) => {
        if (!active) return;
        if (result.status === "error") throw new Error(result.error);
        setIsReturningUser(result.data.is_returning_user);
        setOnboardingStep(result.data.step);
      })
      .catch((error) => {
        if (!active) return;
        console.error("Failed to resolve onboarding state:", error);
        setOnboardingStep("accessibility");
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (
      onboardingStep !== "done" ||
      !settings ||
      hasInitializedRuntime.current
    ) {
      return;
    }
    hasInitializedRuntime.current = true;
    void Promise.all([
      commands.initializeEnigo(),
      commands.initializeShortcuts(),
      refreshAudioDevices(),
      refreshOutputDevices(),
    ]).catch((error) => {
      console.warn("Failed to initialize UI 2.0 runtime:", error);
    });
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices, settings]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);
      if (isDebugShortcut) {
        event.preventDefault();
        void updateSetting("debug_mode", !(settings?.debug_mode ?? false));
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [settings?.debug_mode, updateSetting]);

  const handleAccessibilityComplete = async () => {
    setOnboardingStep(
      await commands.onboardingStepAfterPermissions(isReturningUser),
    );
  };

  if (onboardingStep === null) return null;
  if (onboardingStep === "accessibility") {
    return (
      <>
        <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />
        <Toaster theme={isDark ? "dark" : "light"} />
      </>
    );
  }
  if (onboardingStep === "model") {
    return (
      <>
        <Onboarding onModelSelected={() => setOnboardingStep("done")} />
        <Toaster theme={isDark ? "dark" : "light"} />
      </>
    );
  }

  return (
    <div
      className={`app grain-root${routeUsesCompactGlobalRail(route) ? " notes-mode" : ""}`}
      data-global-rail={
        routeUsesCompactGlobalRail(route) ? "compact" : "expanded"
      }
      data-theme={isDark ? "dark" : "light"}
    >
      <IconSprite />
      <WindowChrome route={route} />
      <Sidebar route={route} />
      <main className="main">
        {route.page === "history" ? (
          <HistorySettings variant="next" controller={history} />
        ) : route.page === "notes" ? (
          <NotesPage />
        ) : route.page === "settings" ? (
          <SettingsPage section={route.section} />
        ) : route.page === "tools" ? (
          <ToolsPage section={route.section} />
        ) : route.page === "extensions" ? (
          <ExtensionsPage view={route.view} />
        ) : route.page === "extension-settings" ? (
          <ExtensionSettingsPage extensionId={route.extensionId} />
        ) : (
          <OverviewPage history={history} />
        )}
      </main>
      <Toaster theme={isDark ? "dark" : "light"} />
    </div>
  );
}

export default function GrainApp() {
  return (
    <ThemeProvider>
      <NextShell />
    </ThemeProvider>
  );
}
