import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
  checkMicrophonePermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { Check, Keyboard, Loader2, Mic, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { commands, events } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import "./onboarding.css";

interface AccessibilityOnboardingProps {
  onComplete: () => void;
}

type PermissionStatus = "checking" | "needed" | "waiting" | "granted";
type PermissionPlatform = "macos" | "windows" | "other";
type MicrophoneTestStatus =
  | "idle"
  | "starting"
  | "listening"
  | "success"
  | "too-quiet"
  | "too-loud"
  | "no-signal"
  | "error";

interface PermissionsState {
  accessibility: PermissionStatus;
  microphone: PermissionStatus;
}

const METER_BAR_COUNT = 34;
const TEST_DURATION_MS = 5000;
const ACTIVE_SPEECH_FLOOR_DBFS = -48;
const QUIET_SPEECH_DBFS = -34;
const LOUD_SPEECH_DBFS = -8;
const CLIPPING_PEAK_DBFS = -0.5;
const MIN_ACTIVE_FRAMES = 12;

const EMPTY_LEVELS = Array.from({ length: METER_BAR_COUNT }, () => 0);

const normalizeLevels = (levels: number[]): number[] => {
  if (levels.length === 0) return EMPTY_LEVELS;
  return Array.from({ length: METER_BAR_COUNT }, (_, index) => {
    const sourceIndex = Math.min(
      levels.length - 1,
      Math.floor((index / METER_BAR_COUNT) * levels.length),
    );
    return Math.max(0, Math.min(1, levels[sourceIndex] ?? 0));
  });
};

const AccessibilityOnboarding: React.FC<AccessibilityOnboardingProps> = ({
  onComplete,
}) => {
  const { t } = useTranslation();
  const {
    audioDevices,
    getSetting,
    updateSetting,
    refreshAudioDevices,
    refreshOutputDevices,
    isUpdating,
  } = useSettings();

  const [permissionPlatform, setPermissionPlatform] =
    useState<PermissionPlatform | null>(null);
  const [permissions, setPermissions] = useState<PermissionsState>({
    accessibility: "checking",
    microphone: "checking",
  });
  const [testStatus, setTestStatus] = useState<MicrophoneTestStatus>("idle");
  const [meterLevels, setMeterLevels] = useState<number[]>(EMPTY_LEVELS);
  const [secondsRemaining, setSecondsRemaining] = useState(5);
  const [measuredLevel, setMeasuredLevel] = useState<number | null>(null);
  const [isCompleting, setIsCompleting] = useState(false);

  const permissionPollingRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );
  const testTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const progressIntervalRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );
  const meterUnlistenRef = useRef<(() => void) | null>(null);
  const rmsSamplesRef = useRef<number[]>([]);
  const peakDbfsRef = useRef(-80);

  const isMacOS = permissionPlatform === "macos";
  const isWindows = permissionPlatform === "windows";
  const microphoneGranted = permissions.microphone === "granted";
  const accessibilityGranted =
    !isMacOS || permissions.accessibility === "granted";
  const testPassed = testStatus === "success";

  const selectedMicrophoneSetting =
    getSetting("selected_microphone") ?? "Default";
  const selectedMicrophone =
    selectedMicrophoneSetting.toLowerCase() === "default"
      ? "Default"
      : selectedMicrophoneSetting;

  const stopPermissionPolling = useCallback(() => {
    if (permissionPollingRef.current) {
      clearInterval(permissionPollingRef.current);
      permissionPollingRef.current = null;
    }
  }, []);

  const stopMicrophoneTest = useCallback(async (resetMeter = true) => {
    if (testTimeoutRef.current) {
      clearTimeout(testTimeoutRef.current);
      testTimeoutRef.current = null;
    }
    if (progressIntervalRef.current) {
      clearInterval(progressIntervalRef.current);
      progressIntervalRef.current = null;
    }
    meterUnlistenRef.current?.();
    meterUnlistenRef.current = null;
    rmsSamplesRef.current = [];
    peakDbfsRef.current = -80;
    if (resetMeter) setMeterLevels(EMPTY_LEVELS);
    try {
      await commands.stopOnboardingMicrophoneTest();
    } catch (error) {
      console.warn("Failed to stop onboarding microphone test:", error);
    }
  }, []);

  const hasWindowsMicrophoneAccess = useCallback(async (): Promise<boolean> => {
    const status = await commands.getWindowsMicrophonePermissionStatus();
    return !status.supported || status.overall_access !== "denied";
  }, []);

  useEffect(() => {
    let active = true;
    const currentPlatform = platform();
    const nextPlatform: PermissionPlatform =
      currentPlatform === "macos"
        ? "macos"
        : currentPlatform === "windows"
          ? "windows"
          : "other";

    setPermissionPlatform(nextPlatform);

    const checkInitialPermissions = async () => {
      if (nextPlatform === "other") {
        if (!active) return;
        setPermissions({ accessibility: "granted", microphone: "granted" });
        await refreshAudioDevices();
        return;
      }

      if (nextPlatform === "macos") {
        try {
          const [hasAccessibility, hasMicrophone] = await Promise.all([
            checkAccessibilityPermission(),
            checkMicrophonePermission(),
          ]);
          if (!active) return;
          setPermissions({
            accessibility: hasAccessibility ? "granted" : "needed",
            microphone: hasMicrophone ? "granted" : "needed",
          });
          if (hasMicrophone) await refreshAudioDevices();
        } catch (error) {
          if (!active) return;
          console.error("Failed to check macOS permissions:", error);
          setPermissions({ accessibility: "needed", microphone: "needed" });
          toast.error(t("onboarding.permissions.errors.checkFailed"));
        }
        return;
      }

      try {
        const hasMicrophone = await hasWindowsMicrophoneAccess();
        if (!active) return;
        setPermissions({
          accessibility: "granted",
          microphone: hasMicrophone ? "granted" : "needed",
        });
        if (hasMicrophone) await refreshAudioDevices();
      } catch (error) {
        if (!active) return;
        console.warn("Failed to check Windows microphone permission:", error);
        setPermissions({ accessibility: "granted", microphone: "granted" });
        await refreshAudioDevices();
      }
    };

    void checkInitialPermissions();
    return () => {
      active = false;
    };
  }, [hasWindowsMicrophoneAccess, refreshAudioDevices, t]);

  const startPermissionPolling = useCallback(
    (target: "microphone" | "accessibility") => {
      stopPermissionPolling();
      permissionPollingRef.current = setInterval(async () => {
        try {
          if (isWindows) {
            const granted = await hasWindowsMicrophoneAccess();
            if (!granted) return;
            setPermissions((current) => ({
              ...current,
              microphone: "granted",
            }));
            stopPermissionPolling();
            await refreshAudioDevices();
            return;
          }

          const [hasAccessibility, hasMicrophone] = await Promise.all([
            checkAccessibilityPermission(),
            checkMicrophonePermission(),
          ]);
          setPermissions({
            accessibility: hasAccessibility ? "granted" : "needed",
            microphone: hasMicrophone ? "granted" : "needed",
          });

          const targetGranted =
            target === "microphone" ? hasMicrophone : hasAccessibility;
          if (!targetGranted) return;
          stopPermissionPolling();
          if (hasMicrophone) await refreshAudioDevices();
        } catch (error) {
          console.error("Failed while waiting for permission:", error);
          stopPermissionPolling();
          toast.error(t("onboarding.permissions.errors.checkFailed"));
        }
      }, 900);
    },
    [
      hasWindowsMicrophoneAccess,
      isWindows,
      refreshAudioDevices,
      stopPermissionPolling,
      t,
    ],
  );

  useEffect(() => {
    return () => {
      stopPermissionPolling();
      if (testTimeoutRef.current) clearTimeout(testTimeoutRef.current);
      if (progressIntervalRef.current)
        clearInterval(progressIntervalRef.current);
      meterUnlistenRef.current?.();
      void commands.stopOnboardingMicrophoneTest();
    };
  }, [stopPermissionPolling]);

  const handleGrantMicrophone = async () => {
    try {
      if (isWindows) {
        await commands.openMicrophonePrivacySettings();
      } else {
        await requestMicrophonePermission();
      }
      setPermissions((current) => ({
        ...current,
        microphone: "waiting",
      }));
      startPermissionPolling("microphone");
    } catch (error) {
      console.error("Failed to request microphone permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const handleGrantAccessibility = async () => {
    try {
      await requestAccessibilityPermission();
      setPermissions((current) => ({
        ...current,
        accessibility: "waiting",
      }));
      startPermissionPolling("accessibility");
    } catch (error) {
      console.error("Failed to request accessibility permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const handleMicrophoneChange = async (
    event: React.ChangeEvent<HTMLSelectElement>,
  ) => {
    await stopMicrophoneTest();
    setTestStatus("idle");
    setMeasuredLevel(null);
    await updateSetting("selected_microphone", event.target.value);
  };

  const runMicrophoneTest = useCallback(async () => {
    if (!microphoneGranted || testStatus === "starting") return;

    await stopMicrophoneTest();
    setTestStatus("starting");
    setMeasuredLevel(null);
    setSecondsRemaining(5);
    setMeterLevels(EMPTY_LEVELS);

    try {
      meterUnlistenRef.current = await events.onboardingMicrophoneLevel.listen(
        (event) => {
          const nextLevels = normalizeLevels(event.payload.levels);
          setMeterLevels(nextLevels);
          rmsSamplesRef.current.push(event.payload.rms_dbfs);
          peakDbfsRef.current = Math.max(
            peakDbfsRef.current,
            event.payload.peak_dbfs,
          );
        },
      );

      const result =
        await commands.startOnboardingMicrophoneTest(selectedMicrophone);
      if (result.status === "error") throw new Error(result.error);

      setTestStatus("listening");
      const startedAt = Date.now();
      progressIntervalRef.current = setInterval(() => {
        const remaining = Math.max(
          0,
          Math.ceil((TEST_DURATION_MS - (Date.now() - startedAt)) / 1000),
        );
        setSecondsRemaining(remaining);
      }, 100);
      testTimeoutRef.current = setTimeout(() => {
        const activeSamples = rmsSamplesRef.current
          .filter((level) => level >= ACTIVE_SPEECH_FLOOR_DBFS)
          .sort((left, right) => left - right);

        if (activeSamples.length < MIN_ACTIVE_FRAMES) {
          setTestStatus("no-signal");
          setMeasuredLevel(null);
        } else {
          const representativeLevel =
            activeSamples[Math.floor((activeSamples.length - 1) * 0.75)];
          setMeasuredLevel(representativeLevel);
          if (
            peakDbfsRef.current >= CLIPPING_PEAK_DBFS ||
            representativeLevel >= LOUD_SPEECH_DBFS
          ) {
            setTestStatus("too-loud");
          } else if (representativeLevel < QUIET_SPEECH_DBFS) {
            setTestStatus("too-quiet");
          } else {
            setTestStatus("success");
          }
        }
        setSecondsRemaining(0);
        void stopMicrophoneTest();
      }, TEST_DURATION_MS);
    } catch (error) {
      await stopMicrophoneTest();
      console.error("Microphone test failed:", error);
      setTestStatus("error");
    }
  }, [microphoneGranted, selectedMicrophone, stopMicrophoneTest, testStatus]);

  const completeMicrophoneStep = async (allowUntested = false) => {
    if (
      !microphoneGranted ||
      !accessibilityGranted ||
      (!allowUntested && !testPassed)
    ) {
      return;
    }
    setIsCompleting(true);
    try {
      await stopMicrophoneTest();
      await Promise.all([refreshAudioDevices(), refreshOutputDevices()]);
      if (isMacOS) {
        await Promise.all([
          commands.initializeEnigo(),
          commands.initializeShortcuts(),
        ]);
      }
      onComplete();
    } catch (error) {
      console.error("Failed to finish microphone setup:", error);
      toast.error(t("onboarding.setup.microphone.errors.continue"));
      setIsCompleting(false);
    }
  };

  const isChecking =
    permissionPlatform === null || permissions.microphone === "checking";
  const isTesting = testStatus === "starting" || testStatus === "listening";

  const statusCopy = (() => {
    switch (testStatus) {
      case "starting":
        return {
          title: t("onboarding.setup.microphone.starting"),
          detail: t("onboarding.setup.microphone.startingDetail"),
        };
      case "listening":
        return {
          title: t("onboarding.setup.microphone.listening"),
          detail: t("onboarding.setup.microphone.listeningDetail"),
        };
      case "success":
        return {
          title: t("onboarding.setup.microphone.success"),
          detail: t("onboarding.setup.microphone.successDetail"),
        };
      case "too-quiet":
        return {
          title: t("onboarding.setup.microphone.tooQuiet"),
          detail: t("onboarding.setup.microphone.tooQuietDetail"),
        };
      case "too-loud":
        return {
          title: t("onboarding.setup.microphone.tooLoud"),
          detail: t("onboarding.setup.microphone.tooLoudDetail"),
        };
      case "no-signal":
        return {
          title: t("onboarding.setup.microphone.noSignal"),
          detail: t("onboarding.setup.microphone.noSignalDetail"),
        };
      case "error":
        return {
          title: t("onboarding.setup.microphone.error"),
          detail: t("onboarding.setup.microphone.errorDetail"),
        };
      default:
        return {
          title: t("onboarding.setup.microphone.test"),
          detail: t("onboarding.setup.microphone.testDetail"),
        };
    }
  })();

  const primaryAction = () => {
    if (!microphoneGranted) return void handleGrantMicrophone();
    if (!testPassed) return void runMicrophoneTest();
    if (!accessibilityGranted) return void handleGrantAccessibility();
    return void completeMicrophoneStep();
  };

  const skipMicrophoneTest = async () => {
    if (!microphoneGranted) return void handleGrantMicrophone();
    if (!accessibilityGranted) return void handleGrantAccessibility();
    return void completeMicrophoneStep(true);
  };

  const primaryLabel = !microphoneGranted
    ? permissions.microphone === "waiting"
      ? t("onboarding.setup.microphone.waitingPermission")
      : t("onboarding.setup.microphone.allow")
    : !testPassed
      ? isTesting
        ? t("onboarding.setup.microphone.listening")
        : t("onboarding.setup.microphone.testAction")
      : !accessibilityGranted
        ? permissions.accessibility === "waiting"
          ? t("onboarding.setup.microphone.waitingPermission")
          : t("onboarding.setup.microphone.enableShortcuts")
        : t("onboarding.setup.continue");

  const hasLevelResult =
    testStatus === "success" ||
    testStatus === "too-quiet" ||
    testStatus === "too-loud";
  const levelMarker =
    measuredLevel === null
      ? 0
      : Math.max(0, Math.min(100, ((measuredLevel + 48) / 45) * 100));

  return (
    <div className="onboarding-shell">
      <header className="onboarding-topbar">
        <div
          className="onboarding-brand"
          aria-label={t("onboarding.setup.brand")}
        >
          <strong>{t("onboarding.setup.brand")}</strong>
          <span>{t("onboarding.setup.label")}</span>
        </div>

        <ol
          className="onboarding-stepper"
          aria-label={t("onboarding.setup.progress")}
        >
          {(["microphone", "modes", "models", "try"] as const).map(
            (step, index) => (
              <li
                key={step}
                className={index === 0 ? "active" : undefined}
                aria-current={index === 0 ? "step" : undefined}
              >
                <span className="onboarding-stepper-line" aria-hidden="true" />
                <span>{t(`onboarding.setup.steps.${step}`)}</span>
              </li>
            ),
          )}
        </ol>

        <button
          type="button"
          className="onboarding-skip"
          disabled={
            isChecking ||
            isTesting ||
            isCompleting ||
            permissions.microphone === "waiting" ||
            permissions.accessibility === "waiting"
          }
          onClick={skipMicrophoneTest}
        >
          {t("onboarding.setup.skipTest")}
        </button>
      </header>

      <main className="onboarding-stage">
        {isChecking ? (
          <div className="onboarding-loading" role="status">
            <Loader2 aria-hidden="true" />
            <span>{t("onboarding.setup.microphone.checking")}</span>
          </div>
        ) : (
          <section className="onboarding-microphone-step">
            <div className="onboarding-heading">
              <h1>{t("onboarding.setup.microphone.title")}</h1>
              <p>{t("onboarding.setup.microphone.description")}</p>
            </div>

            <label
              className="onboarding-field-label"
              htmlFor="onboarding-microphone"
            >
              {t("onboarding.setup.microphone.inputLabel")}
            </label>
            <select
              id="onboarding-microphone"
              className="onboarding-select"
              value={selectedMicrophone}
              disabled={
                !microphoneGranted ||
                isTesting ||
                isUpdating("selected_microphone")
              }
              onChange={handleMicrophoneChange}
            >
              {audioDevices.map((device) => (
                <option
                  key={`${device.index}-${device.name}`}
                  value={device.name}
                >
                  {device.name}
                </option>
              ))}
            </select>

            {!microphoneGranted ? (
              <div className="onboarding-permission" role="status">
                <span className="onboarding-permission-icon">
                  <Mic aria-hidden="true" />
                </span>
                <span>
                  <strong>
                    {t("onboarding.permissions.microphone.title")}
                  </strong>
                  <small>
                    {t("onboarding.permissions.microphone.description")}
                  </small>
                </span>
                <button
                  type="button"
                  disabled={permissions.microphone === "waiting"}
                  onClick={handleGrantMicrophone}
                >
                  {permissions.microphone === "waiting" ? (
                    <Loader2 className="spin" aria-hidden="true" />
                  ) : null}
                  {primaryLabel}
                </button>
              </div>
            ) : (
              <div
                className={`onboarding-mic-panel ${testStatus}`}
                aria-live="polite"
              >
                <button
                  type="button"
                  className="onboarding-mic-button"
                  disabled={isTesting}
                  aria-label={
                    testPassed
                      ? t("onboarding.setup.microphone.retest")
                      : t("onboarding.setup.microphone.testAction")
                  }
                  onClick={runMicrophoneTest}
                >
                  {testStatus === "success" ? (
                    <Check aria-hidden="true" />
                  ) : testStatus === "no-signal" ||
                    testStatus === "too-quiet" ||
                    testStatus === "too-loud" ||
                    testStatus === "error" ? (
                    <RotateCcw aria-hidden="true" />
                  ) : isTesting ? (
                    <span className="onboarding-mic-pulse">
                      <Mic aria-hidden="true" />
                    </span>
                  ) : (
                    <Mic aria-hidden="true" />
                  )}
                </button>

                <div className="onboarding-mic-main">
                  <div className="onboarding-mic-copy">
                    <strong>{statusCopy.title}</strong>
                    <span>{statusCopy.detail}</span>
                  </div>
                  {hasLevelResult ? (
                    <div className="onboarding-level-result">
                      <div
                        className="onboarding-level-track"
                        aria-hidden="true"
                      >
                        <span className="quiet" />
                        <span className="balanced" />
                        <span className="loud" />
                        <i style={{ left: `${levelMarker}%` }} />
                      </div>
                      <div
                        className="onboarding-level-labels"
                        aria-hidden="true"
                      >
                        <span>{t("onboarding.setup.microphone.quiet")}</span>
                        <span>{t("onboarding.setup.microphone.balanced")}</span>
                        <span>{t("onboarding.setup.microphone.loud")}</span>
                      </div>
                    </div>
                  ) : (
                    <div className="onboarding-mic-meter" aria-hidden="true">
                      {meterLevels.map((level, index) => (
                        <i
                          key={index}
                          style={{ height: `${8 + level * 34}px` }}
                        />
                      ))}
                    </div>
                  )}
                  <div className="onboarding-mic-result">
                    {testStatus === "success" ? (
                      <span className="onboarding-success-mark">
                        <Check aria-hidden="true" />
                      </span>
                    ) : null}
                    <span>
                      {testStatus === "success"
                        ? t("onboarding.setup.microphone.inputGood")
                        : isTesting
                          ? t("onboarding.setup.microphone.checkingInput", {
                              count: secondsRemaining,
                            })
                          : testStatus === "too-quiet" ||
                              testStatus === "too-loud"
                            ? t("onboarding.setup.microphone.adjustAndRetry")
                            : t("onboarding.setup.microphone.ready")}
                    </span>
                  </div>
                </div>
              </div>
            )}

            {testPassed && !accessibilityGranted ? (
              <div className="onboarding-permission compact" role="status">
                <span className="onboarding-permission-icon">
                  <Keyboard aria-hidden="true" />
                </span>
                <span>
                  <strong>
                    {t("onboarding.setup.microphone.shortcutsTitle")}
                  </strong>
                  <small>
                    {t("onboarding.setup.microphone.shortcutsDescription")}
                  </small>
                </span>
                <button
                  type="button"
                  disabled={permissions.accessibility === "waiting"}
                  onClick={handleGrantAccessibility}
                >
                  {permissions.accessibility === "waiting" ? (
                    <Loader2 className="spin" aria-hidden="true" />
                  ) : null}
                  {primaryLabel}
                </button>
              </div>
            ) : null}
          </section>
        )}
      </main>

      <footer className="onboarding-footer">
        <div className="onboarding-footer-inner">
          <span>{t("onboarding.setup.microphone.footerHint")}</span>
          <button
            type="button"
            className="onboarding-primary"
            disabled={
              isChecking ||
              isCompleting ||
              isTesting ||
              permissions.microphone === "waiting" ||
              permissions.accessibility === "waiting"
            }
            onClick={primaryAction}
          >
            {isCompleting ? (
              <Loader2 className="spin" aria-hidden="true" />
            ) : null}
            {primaryLabel}
          </button>
        </div>
      </footer>
    </div>
  );
};

export default AccessibilityOnboarding;
