import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  Archive,
  Blocks,
  CircleHelp,
  Clock3,
  FileText,
  History,
  Home,
  Moon,
  NotebookPen,
  Settings,
  Sparkles,
  Sun,
  Waves,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Toaster } from "sonner";
import TitleBar from "@/components/titlebar";
import { HistorySettings } from "@/components/settings/history/HistorySettings";
import Onboarding, { AccessibilityOnboarding } from "@/components/onboarding";
import { commands, type OnboardingStep } from "@/bindings";
import { ThemeProvider, useTheme } from "@/contexts/ThemeContext";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { routeFromHash, type AppRoute } from "./navigation";
import { SettingsPage } from "./pages/SettingsPage";
import "./next.css";

let onboardingResolution: ReturnType<
  typeof commands.resolveOnboardingState
> | null = null;

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

const GRAIN_LINES = Array.from({ length: 36 }, (_, index) => ({
  height: `${0.7 + (index % 9) * 0.72}rem`,
  opacity: 0.3 + (index % 5) * 0.12,
  offset: `${(index % 4) * 0.38}rem`,
}));

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

const navItems = [
  { id: "overview", icon: Home, enabled: true },
  { id: "notes", icon: NotebookPen, enabled: false },
  { id: "history", icon: History, enabled: true },
  { id: "settings", icon: Settings, enabled: true },
  { id: "extensions", icon: Blocks, enabled: false },
  { id: "about", icon: CircleHelp, enabled: false },
] as const;

function GrainMark() {
  return (
    <span className="next-mark" aria-hidden="true">
      {Array.from({ length: 9 }, (_, index) => (
        <i key={index} />
      ))}
    </span>
  );
}

function Sidebar({ route }: { route: AppRoute }) {
  const { t } = useTranslation();
  const { mode, isDark, setMode } = useTheme();
  const currentModel = useModelStore((state) => state.currentModel);
  const models = useModelStore((state) => state.models);
  const loading = useModelStore((state) => state.loading);
  const modelName = useMemo(
    () =>
      models.find((model) => model.id === currentModel)?.name ?? currentModel,
    [currentModel, models],
  );

  const cycleTheme = () => {
    if (mode === "system") setMode("light");
    else if (mode === "light") setMode("dark");
    else setMode("system");
  };

  return (
    <aside className="next-sidebar">
      <div className="next-brand">
        <GrainMark />
        <div>
          <strong>{t("ui2.brand")}</strong>
          <span>{t("ui2.workspace")}</span>
        </div>
      </div>

      <nav className="next-nav" aria-label={t("ui2.navigation")}>
        {navItems.map(({ id, icon: Icon, enabled }) => {
          const active = id === route.page;
          const label = t(`ui2.nav.${id}`);

          return enabled ? (
            <a
              key={id}
              href={id === "settings" ? "#/settings/general" : `#/${id}`}
              className="next-nav-item"
              aria-current={active ? "page" : undefined}
            >
              <Icon aria-hidden="true" size={17} strokeWidth={1.7} />
              <span>{label}</span>
              {active && <i aria-hidden="true" />}
            </a>
          ) : (
            <span
              key={id}
              className="next-nav-item is-disabled"
              aria-disabled="true"
              title={t("ui2.unavailable")}
            >
              <Icon aria-hidden="true" size={17} strokeWidth={1.7} />
              <span>{label}</span>
            </span>
          );
        })}
      </nav>

      <div className="next-sidebar-footer">
        <div className="next-readiness">
          <i className={loading ? "is-loading" : ""} aria-hidden="true" />
          <div>
            <strong>
              {loading ? t("ui2.model.checking") : t("ui2.model.ready")}
            </strong>
            <span>{modelName || t("ui2.model.local")}</span>
          </div>
        </div>
        <button type="button" className="next-theme" onClick={cycleTheme}>
          {isDark ? <Moon size={15} /> : <Sun size={15} />}
          <span>{t(`ui2.theme.${mode}`)}</span>
        </button>
      </div>
    </aside>
  );
}

function AmbientField() {
  return (
    <div className="next-ambient" aria-hidden="true">
      <div className="next-orbit next-orbit-a" />
      <div className="next-orbit next-orbit-b" />
      <div className="next-grain-lines">
        {GRAIN_LINES.map((line, index) => (
          <i
            key={index}
            style={
              {
                "--grain-height": line.height,
                "--grain-opacity": line.opacity,
                "--grain-offset": line.offset,
              } as CSSProperties
            }
          />
        ))}
      </div>
    </div>
  );
}

function OverviewPage() {
  const { t } = useTranslation();

  return (
    <div className="next-page next-overview">
      <header className="next-page-head">
        <div>
          <p>{t("ui2.overview.eyebrow")}</p>
          <h1>{t("ui2.overview.title")}</h1>
        </div>
        <span className="next-demo-label">{t("ui2.demo")}</span>
      </header>

      <section className="next-hero" aria-labelledby="overview-hero-title">
        <AmbientField />
        <div className="next-hero-copy">
          <span className="next-local-state">
            <i aria-hidden="true" /> {t("ui2.overview.local")}
          </span>
          <h2 id="overview-hero-title">{t("ui2.overview.heroTitle")}</h2>
          <p>{t("ui2.overview.heroBody")}</p>
          <div
            className="next-hero-meta"
            aria-label={t("ui2.overview.previewLabel")}
          >
            <span>
              <Clock3 size={14} /> {t("ui2.overview.previewTime")}
            </span>
            <span>
              <Waves size={14} /> {t("ui2.overview.previewMode")}
            </span>
          </div>
        </div>
      </section>

      <div className="next-overview-lower">
        <section className="next-utilities" aria-labelledby="utilities-title">
          <div className="next-section-title">
            <div>
              <p>{t("ui2.overview.utilitiesEyebrow")}</p>
              <h2 id="utilities-title">{t("ui2.overview.utilities")}</h2>
            </div>
            <span>{t("ui2.presentational")}</span>
          </div>
          <div className="next-utility-list">
            <article>
              <span>
                <Sparkles size={16} />
              </span>
              <div>
                <strong>{t("ui2.overview.postProcessing")}</strong>
                <p>{t("ui2.overview.postProcessingBody")}</p>
              </div>
              <small>{t("ui2.overview.ready")}</small>
            </article>
            <article>
              <span>
                <FileText size={16} />
              </span>
              <div>
                <strong>{t("ui2.overview.notes")}</strong>
                <p>{t("ui2.overview.notesBody")}</p>
              </div>
              <small>{t("ui2.overview.localOnly")}</small>
            </article>
            <article>
              <span>
                <Archive size={16} />
              </span>
              <div>
                <strong>{t("ui2.overview.archive")}</strong>
                <p>{t("ui2.overview.archiveBody")}</p>
              </div>
              <small>{t("ui2.overview.private")}</small>
            </article>
          </div>
        </section>

        <section className="next-recent" aria-labelledby="recent-title">
          <div className="next-section-title">
            <div>
              <p>{t("ui2.overview.recentEyebrow")}</p>
              <h2 id="recent-title">{t("ui2.overview.recent")}</h2>
            </div>
          </div>
          <blockquote>{t("ui2.overview.sampleTranscript")}</blockquote>
          <footer>
            <span>{t("ui2.overview.sampleSource")}</span>
            <time>{t("ui2.overview.sampleTime")}</time>
          </footer>
        </section>
      </div>
    </div>
  );
}

function HistoryPage() {
  const { t } = useTranslation();
  return (
    <div className="next-page next-history">
      <header className="next-page-head">
        <div>
          <p>{t("ui2.history.eyebrow")}</p>
          <h1>{t("ui2.nav.history")}</h1>
        </div>
      </header>
      <HistorySettings />
    </div>
  );
}

function NextShell() {
  const route = useHashRoute();
  const { isDark } = useTheme();
  const { t } = useTranslation();
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
    <div className="next-root" data-theme={isDark ? "dark" : "light"}>
      <div className="next-titlebar">
        <TitleBar />
        <div className="next-title-copy" aria-hidden="true">
          <span>{t("ui2.brand")}</span>
          <i />
          <span>{t(`ui2.nav.${route.page}`)}</span>
        </div>
      </div>
      <Sidebar route={route} />
      <main
        className={`next-main ${route.page === "settings" ? "next-main-settings" : ""}`}
      >
        {route.page === "history" ? (
          <HistoryPage />
        ) : route.page === "settings" ? (
          <SettingsPage section={route.section} />
        ) : (
          <OverviewPage />
        )}
      </main>
      <Toaster theme={isDark ? "dark" : "light"} />
    </div>
  );
}

export default function NextApp() {
  return (
    <ThemeProvider>
      <NextShell />
    </ThemeProvider>
  );
}
