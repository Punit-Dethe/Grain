import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
  checkMicrophonePermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { ArrowRight, Keyboard, Loader2, Mic, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { OnboardingLayout } from "./OnboardingLayout";

interface AccessibilityOnboardingProps {
  onComplete: () => void | Promise<void>;
}
type PermissionStatus = "checking" | "needed" | "waiting" | "granted";
type PermissionPlatform = "macos" | "windows" | "other";
interface PermissionsState {
  accessibility: PermissionStatus;
  microphone: PermissionStatus;
}

export default function AccessibilityOnboarding({
  onComplete,
}: AccessibilityOnboardingProps) {
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
  const [isCompleting, setIsCompleting] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const permissionPollingRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );
  const mountedRef = useRef(true);
  const isMacOS = permissionPlatform === "macos";
  const isWindows = permissionPlatform === "windows";
  const microphoneGranted = permissions.microphone === "granted";
  const accessibilityGranted =
    !isMacOS || permissions.accessibility === "granted";
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
            if (!mountedRef.current) return;
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
          if (!mountedRef.current) return;
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
          if (!mountedRef.current) return;
          console.error("Failed while waiting for permission:", error);
          stopPermissionPolling();
          setPermissions((current) => ({ ...current, [target]: "needed" }));
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
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      stopPermissionPolling();
    };
  }, [stopPermissionPolling]);

  const handleGrantMicrophone = async () => {
    try {
      if (isWindows) {
        await commands.openMicrophonePrivacySettings();
      } else {
        await requestMicrophonePermission();
      }
      if (!mountedRef.current) return;
      setPermissions((current) => ({
        ...current,
        microphone: "waiting",
      }));
      startPermissionPolling("microphone");
    } catch (error) {
      if (!mountedRef.current) return;
      console.error("Failed to request microphone permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const handleGrantAccessibility = async () => {
    try {
      await requestAccessibilityPermission();
      if (!mountedRef.current) return;
      setPermissions((current) => ({
        ...current,
        accessibility: "waiting",
      }));
      startPermissionPolling("accessibility");
    } catch (error) {
      if (!mountedRef.current) return;
      console.error("Failed to request accessibility permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const isChecking =
    permissionPlatform === null ||
    permissions.microphone === "checking" ||
    permissions.accessibility === "checking";
  const completeMicrophoneStep = async () => {
    if (
      !microphoneGranted ||
      !accessibilityGranted ||
      isCompleting ||
      isUpdating("selected_microphone")
    )
      return;
    setIsCompleting(true);
    try {
      await Promise.all([refreshAudioDevices(), refreshOutputDevices()]);
      if (isMacOS) {
        await Promise.all([
          commands.initializeEnigo(),
          commands.initializeShortcuts(),
        ]);
      }
      if (mountedRef.current) await onComplete();
    } catch (error) {
      if (!mountedRef.current) return;
      console.error("Failed to finish microphone setup:", error);
      toast.error(t("onboarding.setup.microphone.errors.continue"));
    } finally {
      if (mountedRef.current) setIsCompleting(false);
    }
  };
  const refreshDevices = async () => {
    setIsRefreshing(true);
    try {
      await refreshAudioDevices();
    } finally {
      if (mountedRef.current) setIsRefreshing(false);
    }
  };

  return (
    <OnboardingLayout
      step={0}
      footer={
        <>
          <span>{t("onboarding.setup.microphone.footerHint")}</span>
          <button
            type="button"
            className="onboarding-primary"
            disabled={
              isChecking ||
              isCompleting ||
              !microphoneGranted ||
              !accessibilityGranted ||
              isUpdating("selected_microphone")
            }
            onClick={completeMicrophoneStep}
          >
            {isCompleting ? (
              <Loader2 className="spin" aria-hidden="true" />
            ) : (
              <ArrowRight aria-hidden="true" />
            )}
            {t("onboarding.setup.continue")}
          </button>
        </>
      }
    >
      <section className="onboarding-microphone-step">
        <div className="onboarding-heading">
          <h1>{t("onboarding.setup.microphone.title")}</h1>
          <p>{t("onboarding.setup.microphone.description")}</p>
        </div>
        <div className="onboarding-device-field">
          <div className="onboarding-field-header">
            <label
              className="onboarding-field-label"
              htmlFor="onboarding-microphone"
            >
              {t("onboarding.setup.microphone.inputLabel")}
            </label>
            <button
              type="button"
              className="onboarding-skip"
              disabled={!microphoneGranted || isRefreshing || isCompleting}
              onClick={refreshDevices}
              aria-label={t("onboarding.setup.microphone.refresh")}
            >
              <RefreshCw
                className={isRefreshing ? "spin" : undefined}
                aria-hidden="true"
              />
              {t("onboarding.setup.microphone.refresh")}
            </button>
          </div>
          <div className="onboarding-device-select">
            <Mic aria-hidden="true" />
            <select
              id="onboarding-microphone"
              className="onboarding-select"
              value={selectedMicrophone}
              disabled={
                !microphoneGranted ||
                isCompleting ||
                isUpdating("selected_microphone")
              }
              onChange={(event) =>
                void updateSetting("selected_microphone", event.target.value)
              }
            >
              {!audioDevices.some(
                (device) => device.name === selectedMicrophone,
              ) && (
                <option value={selectedMicrophone}>
                  {selectedMicrophone === "Default"
                    ? t("onboarding.setup.microphone.systemDefault")
                    : selectedMicrophone}
                </option>
              )}
              {audioDevices.map((device) => (
                <option
                  key={`${device.index}-${device.name}`}
                  value={device.name}
                >
                  {device.name === "Default"
                    ? t("onboarding.setup.microphone.systemDefault")
                    : device.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        {isChecking && (
          <p className="onboarding-support-note" role="status">
            {t("onboarding.setup.microphone.checking")}
          </p>
        )}
        {!isChecking && !microphoneGranted && (
          <div className="onboarding-permission" role="status">
            <Mic aria-hidden="true" />
            <div>
              <strong>{t("onboarding.permissions.microphone.title")}</strong>
              <small>
                {t("onboarding.permissions.microphone.description")}
              </small>
            </div>
            <button
              type="button"
              disabled={permissions.microphone === "waiting"}
              onClick={handleGrantMicrophone}
            >
              {t(
                permissions.microphone === "waiting"
                  ? "onboarding.setup.microphone.waitingPermission"
                  : "onboarding.setup.microphone.allow",
              )}
            </button>
          </div>
        )}
        {!isChecking && !accessibilityGranted && (
          <div className="onboarding-permission" role="status">
            <Keyboard aria-hidden="true" />
            <div>
              <strong>{t("onboarding.setup.microphone.shortcutsTitle")}</strong>
              <small>
                {t("onboarding.setup.microphone.shortcutsDescription")}
              </small>
            </div>
            <button
              type="button"
              disabled={permissions.accessibility === "waiting"}
              onClick={handleGrantAccessibility}
            >
              {t(
                permissions.accessibility === "waiting"
                  ? "onboarding.setup.microphone.waitingPermission"
                  : "onboarding.setup.microphone.enableShortcuts",
              )}
            </button>
          </div>
        )}
      </section>
    </OnboardingLayout>
  );
}
