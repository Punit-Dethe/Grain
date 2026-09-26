import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { WindowChrome } from "./components/WindowChrome";
import { platform } from "@tauri-apps/plugin-os";
import { useTranslation } from "react-i18next";
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
  createOnboardingDraft,
  onboardingModes,
} from "./components/onboarding/onboardingState";
import { OnboardingLayout } from "./components/onboarding/OnboardingLayout";
import {
  hashForRoute,
  routeFromHash,
  routeUsesCompactGlobalRail,
  type AppRoute,
} from "./navigation";
import { SettingsPage } from "./pages/SettingsPage";
import { ToolsPage } from "./pages/ToolsPage";
import { AgentPage } from "./pages/AgentPage";
import { ExtensionsPage, ExtensionSettingsPage } from "./pages/ExtensionsPage";
import { HistoryCard, type HistoryViewMode } from "./history/HistoryCard";
import {
  useHistoryController,
  wasPostProcessRequested,
  type HistoryController,
} from "./history/useHistoryController";
import { OverviewCards } from "./overview/OverviewCards";
import overviewHeroOption2 from "./overview/overview-hero-option-2.webp";
import overviewHeroOption4 from "./overview/studio-feature-agent.webp";
import grainMark from "./branding/grain-mark.png";
import { UpdateNotice } from "@/components/UpdateNotice";
import { QuickPanel } from "./quick-panel/QuickPanel";
import "@fontsource-variable/ibm-plex-sans/wght.css";
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
  brand: "Grain",
  quickPanel: "Quick Search",
  quickPanelShortcut: "Ctrl K",
  original: "Original",
  processed: "AI processed",
  heroTitle: "Speak before the thought disappears.",
  personalize: "Personalize",
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
  | "clock"
  | "sliders"
  | "box"
  | "zap"
  | "agent"
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
      <symbol id="i-agent" viewBox="0 0 24 24">
        <rect x="4" y="8" width="16" height="12" rx="3" />
        <path d="M12 8V4H9M2 12v4M22 12v4M9 13v2M15 13v2" />
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
      <symbol id="i-check" viewBox="0 0 24 24">
        <path d="m5 12 4 4L19 6" />
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

const NAV_GROUPS = [
  {
    label: "Workspace",
    items: [
      { page: "overview", label: "Overview", icon: "home", href: "#/overview" },
      { page: "history", label: "History", icon: "clock", href: "#/history" },
      {
        page: "tools",
        label: "Personalize",
        icon: "zap",
        href: "#/tools/dictionary",
      },
      { page: "agent", label: "Agent", icon: "agent", href: "#/agent" },
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
          <img
            className="grain-mark"
            src={grainMark}
            alt={PROTOTYPE_COPY.brand}
          />
          <strong aria-hidden="true">{PROTOTYPE_COPY.brand}</strong>
        </div>
      </div>
      {NAV_GROUPS.map((group) => (
        <nav className="nav-section" key={group.label}>
          <div className="nav-label">{group.label}</div>
          <div className="nav-list">
            {group.items.map((item) => {
              if (!import.meta.env.DEV && item.page === "extensions")
                return null;
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

const OVERVIEW_HERO_IMAGES = [
  {
    src: overviewHeroOption2,
    alt: "Person running across a vivid mountain landscape",
  },
  {
    src: overviewHeroOption4,
    alt: "Figure in a flower field beneath a ringed planet",
  },
] as const;

function OverviewPage({ history }: { history: HistoryController }) {
  const [mode, setMode] = useState<HistoryViewMode>("original");
  const [heroImageIndex, setHeroImageIndex] = useState(0);

  const processedEntries = history.entries.filter(wasPostProcessRequested);

  // The overview starts with three rows. Load older rows only when the user
  // asks for AI attempts, stopping as soon as three are available.
  useEffect(() => {
    if (
      mode === "processed" &&
      processedEntries.length < 3 &&
      history.hasMore &&
      !history.loading &&
      !history.loadingMore
    ) {
      void history.loadMore();
    }
  }, [
    history.hasMore,
    history.loadMore,
    history.loading,
    history.loadingMore,
    mode,
    processedEntries.length,
  ]);

  // AI view lists attempts, including a failed call whose raw fallback must
  // remain visible. Original view shows every capture.
  const recentEntries = (
    mode === "processed" ? processedEntries : history.entries
  ).slice(0, 3);

  return (
    <section className="page active" data-page-panel="overview">
      <div className="page-wrap wide">
        <div className="hero">
          <img
            className="hero-image"
            src={OVERVIEW_HERO_IMAGES[heroImageIndex].src}
            alt={OVERVIEW_HERO_IMAGES[heroImageIndex].alt}
          />
          <button
            className="hero-image-toggle"
            type="button"
            aria-label={
              heroImageIndex === 0 ? "Show hero image 4" : "Show hero image 2"
            }
            title="Switch hero image"
            onClick={() => setHeroImageIndex((current) => 1 - current)}
          >
            <Icon name="refresh" small />
          </button>
          <div className="hero-content">
            <div className="hero-copy">
              <h2>{PROTOTYPE_COPY.heroTitle}</h2>
            </div>
            <div className="hero-actions">
              <button
                className="button primary"
                type="button"
                onClick={() => {
                  window.location.hash = hashForRoute({
                    page: "tools",
                    section: "dictionary",
                  });
                }}
              >
                {PROTOTYPE_COPY.personalize}
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
  const { t } = useTranslation();
  const route = useHashRoute();
  const history = useHistoryController();
  const { isDark } = useTheme();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [modelDraft, setModelDraft] = useState(createOnboardingDraft);
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

  const availableModes = onboardingModes(
    {
      ...modelDraft,
      selectedModels: {
        standard:
          modelDraft.selectedModels.standard || settings?.selected_model || "",
        streaming:
          modelDraft.selectedModels.streaming ||
          settings?.selected_asr_model ||
          "",
      },
    },
    settings?.translate_to_english,
  );

  if (onboardingStep === null)
    return (
      <OnboardingLayout step={0}>
        <div className="onboarding-loading" role="status">
          {t("onboarding.setup.loading")}
        </div>
      </OnboardingLayout>
    );
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
          draft={modelDraft}
          onDraftChange={setModelDraft}
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
          availableModes={availableModes}
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
          availableModes={availableModes}
          onBack={() => setOnboardingStep("try")}
          onComplete={() => setOnboardingStep("done")}
        />
        <Toaster theme={isDark ? "dark" : "light"} />
      </>
    );
  }

  return (
    <div
      className="app grain-root"
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
        ) : route.page === "settings" ? (
          <SettingsPage section={route.section} />
        ) : route.page === "tools" ? (
          <ToolsPage section={route.section} />
        ) : route.page === "agent" ? (
          <AgentPage />
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
