import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type EmbedModelStatus } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";

export function EmbeddingSettings() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<EmbedModelStatus | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(false);
  const operation = useRef(false);

  useEffect(() => {
    let active = true;
    alive.current = true;
    const unlisten: (() => void)[] = [];
    const refresh = async () => {
      try {
        const value = await commands.grainEmbedModelStatus();
        if (active) setStatus(value);
      } catch (reason) {
        if (active) setError(String(reason));
      }
    };
    const subscribe = async () => {
      const registrations = [
        events.grainEmbedModelProgress.listen(({ payload }) => {
          if (!active) return;
          setStatus("downloading");
          setProgress(payload.percentage);
        }),
        events.grainEmbedModelComplete.listen(() => void refresh()),
        events.grainEmbedModelCancelled.listen(() => void refresh()),
        events.grainEmbedModelError.listen(({ payload }) => {
          if (active) setError(payload);
          void refresh();
        }),
      ];
      await Promise.all(
        registrations.map(async (registration) => {
          try {
            const stop = await registration;
            if (active) unlisten.push(stop);
            else stop();
          } catch (reason) {
            if (active) setError(String(reason));
          }
        }),
      );
      if (active) await refresh();
    };
    void subscribe();
    return () => {
      active = false;
      alive.current = false;
      unlisten.forEach((stop) => stop());
    };
  }, []);

  const manage = async (action: "download" | "cancel" | "uninstall") => {
    // Cancellation remains available while the download command is pending.
    if (action !== "cancel" && operation.current) return;
    if (action !== "cancel") operation.current = true;
    setBusy(true);
    setError(null);
    if (action === "download") {
      setStatus("downloading");
      setProgress(null);
    }
    try {
      const result = await (action === "download"
        ? commands.grainEmbedDownloadModel()
        : action === "cancel"
          ? commands.grainEmbedCancelDownload()
          : commands.grainEmbedUninstallModel());
      if (result.status === "error") throw new Error(result.error);
    } catch (reason) {
      if (alive.current) setError(String(reason));
    } finally {
      if (action !== "cancel") operation.current = false;
      if (alive.current) {
        try {
          const value = await commands.grainEmbedModelStatus();
          if (alive.current) setStatus(value);
        } catch (reason) {
          if (alive.current) setError(String(reason));
        }
        if (alive.current) setBusy(operation.current);
      }
    }
  };

  return (
    <SettingContainer
      title={t("embeddingModel.title")}
      description={t("embeddingModel.description")}
      descriptionMode="tooltip"
      grouped
      layout="horizontal"
    >
      <div className="update-check-row" aria-busy={busy}>
        <span className="update-check-version" role="status">
          {status === "downloading" && progress !== null
            ? t("embeddingModel.progress", { percent: Math.round(progress) })
            : t(`embeddingModel.${status ?? "loading"}`)}
        </span>
        {status === "downloading" ? (
          <button
            className="update-check-btn"
            type="button"
            onClick={() => void manage("cancel")}
          >
            {t("embeddingModel.cancel")}
          </button>
        ) : (
          <button
            className="update-check-btn"
            type="button"
            disabled={busy || status === null}
            onClick={() =>
              void manage(status === "ready" ? "uninstall" : "download")
            }
          >
            {t(
              status === "ready"
                ? "embeddingModel.uninstall"
                : "embeddingModel.download",
            )}
          </button>
        )}
        {error && (
          <span className="update-check-result" data-kind="failed" role="alert">
            {error}
          </span>
        )}
      </div>
    </SettingContainer>
  );
}
