/**
 * The Updates row in Application settings.
 *
 * The automatic-check toggle used to live in Debug, next to log levels and paste
 * delays — a screen most people never open, holding the one switch that decides
 * whether they ever hear about a new release. It belongs here, beside autostart
 * and the tray icon, with the running version visible next to it.
 *
 * "Check now" passes `force: true`, so it answers even when automatic checks are
 * off: the user asking in person is not the same as Grain phoning home.
 */
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { commands } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";

type Result = { kind: "current" | "found" | "failed"; detail: string };

export function UpdatesSection() {
  const [version, setVersion] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<Result | null>(null);

  useEffect(() => {
    let alive = true;
    void getVersion()
      .then((value) => alive && setVersion(value))
      .catch(() => alive && setVersion(null));
    return () => {
      alive = false;
    };
  }, []);

  const check = async () => {
    setChecking(true);
    setResult(null);
    try {
      const response = await commands.checkForUpdate(true);
      if (response.status !== "ok") {
        setResult({ kind: "failed", detail: response.error });
      } else if (response.data) {
        setResult({
          kind: "found",
          detail: `Version ${response.data.version} is available — see the notice in the sidebar.`,
        });
      } else {
        setResult({ kind: "current", detail: "Grain is up to date." });
      }
    } catch (reason) {
      setResult({ kind: "failed", detail: String(reason) });
    } finally {
      setChecking(false);
    }
  };

  return (
    <SettingContainer
      title="Version"
      description="Grain updates in place from its signed release feed. Nothing is downloaded until you choose to install."
      descriptionMode="tooltip"
      grouped
      layout="horizontal"
    >
      <div className="update-check-row">
        <span className="update-check-version">{version ?? "—"}</span>
        <button
          className="update-check-btn"
          type="button"
          disabled={checking}
          onClick={() => void check()}
        >
          {checking ? "Checking…" : "Check now"}
        </button>
        {result && (
          <span className="update-check-result" data-kind={result.kind}>
            {result.detail}
          </span>
        )}
      </div>
    </SettingContainer>
  );
}
