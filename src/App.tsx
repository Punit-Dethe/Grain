import { useEffect, useState, useRef, type ReactNode } from "react";
import { toast, Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { ModelStateEvent, RecordingErrorEvent } from "./lib/types/events";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import Footer from "./components/footer";
import TitleBar from "./components/titlebar";
import Onboarding, { AccessibilityOnboarding } from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { QuickPanel } from "./components/quick-panel/QuickPanel";
import { ScaledStage } from "./components/quick-panel/ScaledStage";
import { useSettings } from "./hooks/useSettings";
import { useSettingsStore } from "./stores/settingsStore";
import { commands } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";
import { ThemeProvider, useTheme } from "./contexts/ThemeContext";

type OnboardingStep = "accessibility" | "model" | "done";

const renderSettingsContent = (section: SidebarSection) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

// Inner component reads the theme context (must be inside ThemeProvider).
function AppInner() {
  const { isSettingsDark } = useTheme();
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  // [GRAIN] Quick panel is a drawer that slides up over the settings surface.
  // Both views are always mounted; only the CSS transform changes.
  const [isQuickOpen, setIsQuickOpen] = useState(false);
  const hasSeededPanel = useRef(false);
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const hasCompletedPostOnboardingInit = useRef(false);

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // Seed the initial panel state from settings
  useEffect(() => {
    if (settings && !hasSeededPanel.current) {
      if (settings.default_panel === "quick_panel") {
        setIsQuickOpen(true);
      }
      // Use requestAnimationFrame to let the non-animated render commit first
      // before we enable transitions
      requestAnimationFrame(() => {
        hasSeededPanel.current = true;
      });
    }
  }, [settings]);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([
        commands.initializeEnigo(),
        commands.initializeShortcuts(),
      ]).catch((e) => {
        console.warn("Failed to initialize:", e);
      });
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  // Listen for recording errors from the backend and show a toast
  useEffect(() => {
    const unlisten = listen<RecordingErrorEvent>("recording-error", (event) => {
      const { error_type, detail } = event.payload;

      if (error_type === "microphone_permission_denied") {
        const currentPlatform = platform();
        const platformKey = `errors.micPermissionDenied.${currentPlatform}`;
        const description = t(platformKey, {
          defaultValue: t("errors.micPermissionDenied.generic"),
        });
        toast.error(t("errors.micPermissionDeniedTitle"), { description });
      } else if (error_type === "no_input_device") {
        toast.error(t("errors.noInputDeviceTitle"), {
          description: t("errors.noInputDevice"),
        });
      } else {
        toast.error(
          t("errors.recordingFailed", { error: detail ?? "Unknown error" }),
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for paste failures and show a toast.
  // The technical error detail is logged to handy.log on the Rust side
  // (see actions.rs `error!("Failed to paste transcription: ...")`),
  // so we show a localized, user-friendly message here instead of the raw error.
  useEffect(() => {
    const unlisten = listen("paste-error", () => {
      toast.error(t("errors.pasteFailedTitle"), {
        description: t("errors.pasteFailed"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for model loading failures and show a toast
  useEffect(() => {
    const unlisten = listen<ModelStateEvent>("model-state-changed", (event) => {
      if (event.payload.event_type === "loading_failed") {
        toast.error(
          t("errors.modelLoadFailed", {
            model:
              event.payload.model_name || t("errors.modelLoadFailedUnknown"),
          }),
          {
            description: event.payload.error,
          },
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  const revealMainWindowForPermissions = async () => {
    try {
      await commands.showMainWindowCommand();
    } catch (e) {
      console.warn("Failed to show main window for permission onboarding:", e);
    }
  };

  const checkOnboardingStatus = async () => {
    try {
      // Check if they have any models available
      const result = await commands.hasAnyModelsAvailable();
      const hasModels = result.status === "ok" && result.data;
      const currentPlatform = platform();

      if (hasModels) {
        // Returning user - check if they need to grant permissions first
        setIsReturningUser(true);

        if (currentPlatform === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check macOS permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        if (currentPlatform === "windows") {
          try {
            const microphoneStatus =
              await commands.getWindowsMicrophonePermissionStatus();
            if (
              microphoneStatus.supported &&
              microphoneStatus.overall_access === "denied"
            ) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check Windows microphone permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        setOnboardingStep("done");
      } else {
        // New user - start full onboarding
        setIsReturningUser(false);
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleAccessibilityComplete = () => {
    // Returning users already have models, skip to main app
    // New users need to select a model
    setOnboardingStep(isReturningUser ? "done" : "model");
  };

  const handleModelSelected = () => {
    // Transition to main app - user has started a download
    setOnboardingStep("done");
  };

  const toaster = (
    <Toaster
      theme="system"
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            "bg-paper-raised border border-line rounded-lg px-4 py-3 flex items-center gap-3 text-sm text-ink",
          title: "font-medium",
          description: "text-ink-soft",
        },
      }}
    />
  );

  // Still checking onboarding status
  if (onboardingStep === null) {
    return null;
  }

  let content: ReactNode;

  if (onboardingStep === "accessibility") {
    content = <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />;
  } else if (onboardingStep === "model") {
    content = <Onboarding onModelSelected={handleModelSelected} />;
  } else {
    content = (
      <div
        dir={direction}
        className="relative h-screen w-screen overflow-hidden select-none cursor-default"
        style={{ backgroundColor: "#0c0b0a" }}
      >
        {/* ── LAYER 1: Settings (always mounted, sits underneath) ── */}
      {/* Chassis fill — no rounded corners. */}
      <div
        className="absolute inset-0 overflow-hidden"
        style={{ backgroundColor: isSettingsDark ? "#0e0c0b" : "#0c0b0a" }}
      >
        <ScaledStage designWidth={1280} designHeight={760}>
          {/* Settings card — no rounded corners. data-theme drives CSS token overrides in App.css. */}
          <div
            className="relative w-full h-full overflow-hidden flex"
            style={{ backgroundColor: "var(--color-paper)" }}
            data-theme={isSettingsDark ? "dark" : "light"}
          >
            {/* TitleBar: absolutely positioned behind the sidebar (z-10).
                The sidebar covers the left portion; the right strip (window
                controls + drag region) stays fully exposed and functional. */}
            <div className="absolute inset-x-0 top-0 z-10">
              <TitleBar />
            </div>

            {/* Sidebar: full height, sits above the TitleBar on the left (z-20). */}
            <div className="relative z-20 h-full shrink-0">
              <Sidebar
                activeSection={currentSection}
                onSectionChange={setCurrentSection}
                onOpenQuickPanel={() => setIsQuickOpen(true)}
              />
            </div>

            {/* Main content: offset from top by TitleBar height (h-9 = 36px). */}
            <div className="flex-1 flex flex-col overflow-hidden pt-9">
              <div className="flex-1 overflow-y-auto">
                {/* key forces remount on tab switch → grain-section-enter animation */}
                <div
                  key={currentSection}
                  className="grain-section-enter flex flex-col items-center px-12 py-9 gap-4"
                >
                  <AccessibilityPermissions />
                  {renderSettingsContent(currentSection)}
                </div>
              </div>
              <Footer />
            </div>
          </div>
        </ScaledStage>
      </div>


      {/* ── LAYER 2: Quick Panel (slides up over settings) ── */}
      <div
        className="absolute inset-0 overflow-hidden"
        style={{
          transform: isQuickOpen ? "translateY(0%)" : "translateY(100%)",
          transition: hasSeededPanel.current
            ? "transform 320ms cubic-bezier(0.22, 1, 0.36, 1)"
            : "none",
          willChange: "transform",
        }}
      >
        <QuickPanel onOpenAdvanced={() => setIsQuickOpen(false)} />
      </div>
    </div>
    );
  }

  return (
    <>
      {toaster}
      {content}
    </>
  );
}

// App: thin wrapper that provides the shared theme context to AppInner.
function App() {
  return (
    <ThemeProvider>
      <AppInner />
    </ThemeProvider>
  );
}

export default App;
