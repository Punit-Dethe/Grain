import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  events,
  type GrainModel,
  type GrainModelStatus,
} from "@/bindings";
import {
  BYTES_PER_MB,
  formatModelSize,
} from "../../handy-models/lib/utils/format";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

/** Live status of one of Grain's own models (talk-back voice, speaker
 * detection): download with progress, cancel, delete. The listener lives only
 * while the component using it is mounted. */
export function useGrainModel(model: GrainModel) {
  const [status, setStatus] = useState<GrainModelStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const result = await commands.grainModelStatus(model);
    if (result.status === "ok") setStatus(result.data);
    else setError(result.error);
  }, [model]);

  useEffect(() => {
    let live = true;
    void refresh();
    const unlisten = events.grainModelProgress.listen(({ payload }) => {
      if (!live || payload.model !== model) return;
      if (payload.done) {
        void refresh();
        return;
      }
      setStatus((current) =>
        current
          ? {
              ...current,
              downloading: true,
              downloaded: payload.downloaded,
              total: payload.total,
            }
          : current,
      );
    });
    return () => {
      live = false;
      void unlisten.then((stop) => stop());
    };
  }, [model, refresh]);

  const run = useCallback(
    async (
      action: () => Promise<
        | { status: "ok"; data: GrainModelStatus }
        | { status: "error"; error: string }
      >,
    ) => {
      setError(null);
      const result = await action();
      if (result.status === "ok") {
        setStatus(result.data);
        return result.data;
      }
      setError(result.error);
      await refresh();
      return null;
    },
    [refresh],
  );

  const download = useCallback(() => {
    setStatus((current) =>
      current ? { ...current, downloading: true } : current,
    );
    return run(() => commands.grainModelDownload(model));
  }, [model, run]);
  const cancel = useCallback(() => commands.grainModelCancel(model), [model]);
  const remove = useCallback(
    () => run(() => commands.grainModelDelete(model)),
    [model, run],
  );

  const percent =
    status && status.total > 0
      ? Math.min(100, Math.round((status.downloaded / status.total) * 100))
      : 0;
  return { status, error, percent, download, cancel, remove };
}

export type GrainModelState = ReturnType<typeof useGrainModel>;

const size = (bytes: number) => formatModelSize(bytes / BYTES_PER_MB);

/** A thin progress bar for a model download. */
export function GrainModelProgressBar({
  percent,
  label,
}: {
  percent: number;
  label: string;
}) {
  return (
    <div
      className="grain-model-progress"
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent}
    >
      <i style={{ transform: `scaleX(${percent / 100})` }} />
    </div>
  );
}

/** One settings row for a Grain model: its size and state, with Download,
 * Cancel or Delete. */
export function GrainModelRow({
  title,
  state,
  onDeleted,
}: {
  title: string;
  state: GrainModelState;
  onDeleted?: () => void;
}) {
  const { t } = useTranslation();
  const { status, error, percent, download, cancel, remove } = state;
  const [busy, setBusy] = useState(false);
  if (!status) return null;

  const description = status.downloading
    ? t("ui2.settings.grainModel.downloading", { percent })
    : status.installed
      ? t("ui2.settings.grainModel.onDisk", { size: size(status.disk_bytes) })
      : t("ui2.settings.grainModel.notDownloaded", {
          size: size(status.total),
        });

  const act = async (
    action: () => Promise<GrainModelStatus | null>,
    after?: () => void,
  ) => {
    setBusy(true);
    try {
      if (await action()) after?.();
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingContainer
      title={title}
      description={error ?? description}
      descriptionMode="inline"
      grouped
    >
      <div className="flex items-center gap-3">
        {status.downloading && (
          <GrainModelProgressBar percent={percent} label={description} />
        )}
        {status.downloading ? (
          <Button variant="secondary" size="sm" onClick={() => void cancel()}>
            {t("ui2.settings.grainModel.cancel")}
          </Button>
        ) : status.installed ? (
          <Button
            variant="danger-ghost"
            size="sm"
            disabled={busy}
            onClick={() => void act(remove, onDeleted)}
          >
            {t("ui2.settings.grainModel.delete")}
          </Button>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => void act(download)}
          >
            {t("ui2.settings.grainModel.download")}
          </Button>
        )}
      </div>
    </SettingContainer>
  );
}
