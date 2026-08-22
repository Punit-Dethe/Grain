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
import Onboarding, {
  AccessibilityOnboarding,
  ModesOnboarding,
  ShortcutsOnboarding,
  TryOnboarding,
} from "@/components/onboarding";
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
import { QuickPanel } from "./quick-panel/QuickPanel";
import "./app.css";

/** ⌘ K on macOS, Ctrl K elsewhere — matches the platform's palette convention. */
function quickPanelShortcutLabel(): string {
  try {
    return platform() === "macos" ? "⌘ K" : "Ctrl K";
  } catch {
    return "Ctrl K";
  }
}

let onboardingResolution: ReturnType<
  typeof commands.resolveOnboardingState
> | null = null;

const PROTOTYPE_COPY = {
  quickPanel: "Quick Search",
  quickPanelShortcut: "Ctrl K",
  original: "Original",
  processed: "AI processed",
  heroTopline: "On-device · Parakeet TDT 0.6B",
  beta: "Beta",
  heroKicker: "Grain is listening when you are",
  heroTitle: "Speak before the thought disappears.",
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

function WindowChrome() {
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
      {/* No page label here: the nav rail already says where you are, and the
          strip reads as one surface with the sidebar without it. */}
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
      { page: "history", label: "History", icon: "clock", href: "#/history" },
      {
        page: "tools",
        label: "Studio",
        icon: "zap",
        href: "#/tools/dictionary",
      },
      { page: "notes", label: "Notes", icon: "note", href: "#/notes" },
    ],
  },
  {
    label: "Configure",
    items: [
      {
        page: "extensions",
        label: "Extensions",
        icon: "box",
        href: "#/extensions/installed",
      },
      {
        page: "settings",
        label: "Settings",
        icon: "sliders",
        href: "#/settings/capture",
      },
    ],
  },
] as const;

function Sidebar({
  route,
  onOpenQuickPanel,
}: {
  route: AppRoute;
  onOpenQuickPanel: () => void;
}) {
  const currentModel = useModelStore((state) => state.currentModel);
  const loadedModelId = useModelStore((state) => state.loadedModelId);
  const models = useModelStore((state) => state.models);
  const loading = useModelStore((state) => state.loading);
  const isModelLoaded = useModelStore((state) => state.isModelLoaded);
  const { settings } = useSettings();
  // Cloud STT rotation replaces the local model entirely: when it is on there
  // is no resident model, so we say "Cloud" and stop — name and load state only
  // mean something for a local model.
  const cloudStt = settings?.stt_smart_rotation === true;

  const modelStatus = useMemo(() => {
    if (cloudStt) return { title: "Cloud model", subtitle: "Cloud" };
    // The manager holds ONE resident model across Standard/Live/Batch. When a
    // model is loaded, show exactly that (so a Live/Batch switch is reflected,
    // not just the Standard slot). When nothing is resident, show the selected
    // Standard model so a fresh switch appears immediately, before it loads.
    const activeId =
      isModelLoaded && loadedModelId ? loadedModelId : currentModel;
    if (loading && !activeId)
      return { title: "Checking model", subtitle: "Checking" };
    const name =
      models.find((model) => model.id === activeId)?.name ?? activeId;
    if (!name) return { title: "No model", subtitle: "Not loaded" };
    return {
      title: name,
      subtitle: `${isModelLoaded ? "Loaded" : "Unloaded"} · Local`,
    };
  }, [cloudStt, loading, currentModel, loadedModelId, models, isModelLoaded]);

  return (
    <aside aria-label="Primary navigation" className="sidebar">
      {/* The sidebar runs the full height of the window and the titlebar is
          transparent, so the brand sits *below* the window-control line rather
          than beside it. The strip's top padding covers that line and stays a
          drag region, so the top-left corner still moves the window. */}
      <div
        className="sidebar-brand"
        data-tauri-drag-region
        style={{ WebkitAppRegion: "drag" } as CSSProperties}
      >
        <div className="grain-wordmark">
          <strong>GRAIN</strong>
          <span className="grain-beta">{PROTOTYPE_COPY.beta}</span>
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
          <strong>{modelStatus.title}</strong>
        </div>
        <p>{modelStatus.subtitle}</p>
      </div>
      <button
        className="quick-panel-button"
        type="button"
        onClick={onOpenQuickPanel}
        aria-label="Open Quick Search"
        title="Quick Search"
      >
        <Icon name="command" />
        <span>{PROTOTYPE_COPY.quickPanel}</span>
        <kbd>{quickPanelShortcutLabel()}</kbd>
      </button>
    </aside>
  );
}

const BRAID_COLORS = [
  [66, 139, 235], // electric blue
  [76, 204, 213], // cyan
  [159, 106, 226], // violet
] as const;

function clamp(v: number, a: number, b: number): number {
  return Math.max(a, Math.min(b, v));
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function smooth(a: number, b: number, x: number): number {
  const v = clamp((x - a) / (b - a), 0, 1);
  return v * v * (3 - 2 * v);
}

function mixRGB(
  a: readonly [number, number, number],
  b: readonly [number, number, number],
  t: number,
): [number, number, number] {
  return [
    Math.round(lerp(a[0], b[0], t)),
    Math.round(lerp(a[1], b[1], t)),
    Math.round(lerp(a[2], b[2], t)),
  ];
}

function colorAt(t: number): [number, number, number] {
  const norm = clamp(t, 0, 1);
  if (norm < 0.5) return mixRGB(BRAID_COLORS[0], BRAID_COLORS[1], norm * 2);
  return mixRGB(BRAID_COLORS[1], BRAID_COLORS[2], (norm - 0.5) * 2);
}

function hash(x: number, y: number): number {
  let n = x * 374761393 + y * 668265263;
  n = (n ^ (n >> 13)) * 1274126177;
  return ((n ^ (n >> 16)) >>> 0) / 4294967295;
}

function DitherCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { isDark } = useTheme();
  const isDarkRef = useRef(isDark);

  useEffect(() => {
    isDarkRef.current = isDark;
  }, [isDark]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    let animationFrame = 0;

    const resize = () => {
      const r = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
      const w = Math.max(1, Math.floor(r.width * dpr));
      const h = Math.max(1, Math.floor(r.height * dpr));

      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
    };

    const draw = (now: number) => {
      resize();

      const t = reducedMotion ? 0 : now / 1000;
      const w = canvas.width;
      const h = canvas.height;
      const dark =
        isDarkRef.current && document.documentElement.dataset.theme !== "light";

      context.clearRect(0, 0, w, h);

      const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
      const cell = 4.1 * dpr;
      const cols = Math.ceil(w / cell);
      const rows = Math.ceil(h / cell);

      // Precompute column-invariant wave trajectory arrays for ~15x performance gain
      const c1Arr = new Float32Array(cols);
      const c2Arr = new Float32Array(cols);
      const c3Arr = new Float32Array(cols);
      const echoCenterArr = new Float32Array(cols);
      const edgeFadeArr = new Float32Array(cols);
      const driftYArr = new Float32Array(cols);
      const xNormArr = new Float32Array(cols);

      const maxCols = Math.max(1, cols - 1);
      for (let gx = 0; gx < cols; gx++) {
        const x = gx / maxCols;
        xNormArr[gx] = x;
        c1Arr[gx] =
          0.23 +
          0.11 * Math.sin(x * 5.5 - t * 0.72) +
          0.018 * Math.sin(x * 14.0 + t * 0.25);
        c2Arr[gx] =
          0.39 +
          0.1 * Math.sin(x * 5.5 - t * 0.72 + 2.05) +
          0.014 * Math.sin(x * 12.0 - t * 0.18);
        c3Arr[gx] = 0.55 + 0.095 * Math.sin(x * 5.5 - t * 0.72 + 4.1);
        echoCenterArr[gx] =
          0.13 + 0.72 * x + 0.05 * Math.sin(x * 8.0 + t * 0.35);
        edgeFadeArr[gx] =
          0.72 + 0.28 * smooth(0.0, 0.12, x) * (1 - smooth(0.9, 1.0, x));
        driftYArr[gx] = Math.cos(t * 0.38 + gx * 0.06) * cell * 0.035;
      }

      const maxRows = Math.max(1, rows - 1);
      const colorDriftT = 0.025 * Math.sin(t * 0.25);
      const baseAlpha = dark ? 0.57 : 0.48;

      for (let gy = 0; gy < rows; gy++) {
        const y = gy / maxRows;
        const driftX = Math.sin(t * 0.45 + gy * 0.075) * cell * 0.045;
        const pulseY = gy * 0.03;

        for (let gx = 0; gx < cols; gx++) {
          const c1 = c1Arr[gx];
          const c2 = c2Arr[gx];
          const c3 = c3Arr[gx];

          const d1 = Math.abs(y - c1);
          const d2 = Math.abs(y - c2);
          const d3 = Math.abs(y - c3);

          // Thin cores + softer atmospheric wings
          const r1 = 1 - smooth(0.004, 0.044, d1);
          const r2 = (1 - smooth(0.004, 0.044, d2)) * 0.92;
          const r3 = (1 - smooth(0.004, 0.044, d3)) * 0.76;

          const wing1 = (1 - smooth(0.04, 0.15, d1)) * 0.16;
          const wing2 = (1 - smooth(0.04, 0.15, d2)) * 0.13;
          const wing3 = (1 - smooth(0.04, 0.15, d3)) * 0.1;

          // Secondary echo arc
          const echoDist = Math.abs(y - echoCenterArr[gx]);
          const echo = (1 - smooth(0.01, 0.055, echoDist)) * 0.22;

          let density = clamp(
            r1 + r2 + r3 + wing1 + wing2 + wing3 + echo,
            0,
            1,
          );

          density *= edgeFadeArr[gx];

          const threshold = hash(gx, gy);
          const active = smooth(threshold - 0.13, threshold + 0.12, density);
          if (active < 0.045) continue;

          // Color calculation
          const x = xNormArr[gx];
          const cp = clamp(0.05 + x * 0.78 + y * 0.18 + colorDriftT, 0, 1);
          const rgb = colorAt(cp);

          let boost = 0.78;
          if (d1 < d2 && d1 < d3) boost = 1;
          else if (d2 < d3) boost = 0.9;

          const alpha = baseAlpha * (0.15 + active * 0.85) * boost;
          const pulse = 1 + 0.035 * Math.sin(t * 1.5 + gx * 0.07 + pulseY);
          const size = cell * (0.11 + active * 0.49) * pulse;

          const px = gx * cell + cell * 0.5 + driftX;
          const py = gy * cell + cell * 0.5 + driftYArr[gx];

          context.fillStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${alpha})`;

          if ((gx * 2 + gy) % 10 === 0) {
            context.beginPath();
            context.arc(px, py, Math.max(size * 0.4, 0.42), 0, Math.PI * 2);
            context.fill();
          } else {
            context.fillRect(
              px - size * 0.5,
              py - size * 0.5,
              Math.max(size, 0.6),
              Math.max(size, 0.6),
            );
          }
        }
      }

      if (!reducedMotion) {
        animationFrame = requestAnimationFrame(draw);
      }
    };

    animationFrame = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(animationFrame);
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
          <DitherCanvas />
          <div className="hero-content">
            <div className="hero-copy">
              <h2>{PROTOTYPE_COPY.heroTitle}</h2>
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
          </div>
        </div>
        <OverviewCards />

        <div className="section-head transcript-section-head">
          <div>
            <h2>{PROTOTYPE_COPY.recent}</h2>
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
  const [quickPanelOpen, setQuickPanelOpen] = useState(false);

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

  // ⌘/Ctrl-K toggles the Quick Panel from anywhere in the app.
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setQuickPanelOpen((prev) => !prev);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

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
  if (onboardingStep === "modes") {
    return (
      <>
        <ModesOnboarding
          onBack={() => setOnboardingStep("accessibility")}
          onComplete={() => setOnboardingStep("model")}
        />
        <Toaster theme={isDark ? "dark" : "light"} />
      </>
    );
  }
  if (onboardingStep === "model") {
    return (
      <>
        <Onboarding
          onBack={() => setOnboardingStep("modes")}
          onModelSelected={() => setOnboardingStep("try")}
        />
        <Toaster theme={isDark ? "dark" : "light"} />
      </>
    );
  }
  if (onboardingStep === "try") {
    return (
      <>
        <TryOnboarding
          onBack={() => setOnboardingStep("model")}
          onComplete={() => setOnboardingStep("shortcuts")}
        />
        <Toaster theme={isDark ? "dark" : "light"} />
      </>
    );
  }
  if (onboardingStep === "shortcuts") {
    return (
      <>
        <ShortcutsOnboarding
          onBack={() => setOnboardingStep("try")}
          onComplete={() => setOnboardingStep("done")}
        />
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
      <WindowChrome />
      <Sidebar route={route} onOpenQuickPanel={() => setQuickPanelOpen(true)} />
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
      <QuickPanel
        open={quickPanelOpen}
        onClose={() => setQuickPanelOpen(false)}
      />
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
