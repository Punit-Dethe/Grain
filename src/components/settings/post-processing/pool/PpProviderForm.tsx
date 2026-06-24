import React, { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCcw } from "lucide-react";
import { Input } from "../../../ui/Input";
import { Button } from "../../../ui/Button";
import { BUILTIN_PP_IDS } from "./usePpPool";
import type { PostProcessProvider } from "@/bindings";

interface PpProviderFormProps {
  existing?: PostProcessProvider;
  /** Current model id for the existing provider (from the pool's model map). */
  existingModel?: string;
  onSave: (
    provider: PostProcessProvider,
    apiKey: string | null,
    model: string | null,
  ) => Promise<void>;
  onFetchModels?: (id: string) => Promise<string[]>;
  onCancel: () => void;
}

const newId = (): string =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? `pp_${crypto.randomUUID()}`
    : `pp_${Date.now()}_${Math.random().toString(36).slice(2)}`;

export const PpProviderForm: React.FC<PpProviderFormProps> = ({
  existing,
  existingModel,
  onSave,
  onFetchModels,
  onCancel,
}) => {
  const { t } = useTranslation();
  const isEdit = !!existing;
  const datalistId = useId();

  // Built-in providers keep their fixed endpoint unless they explicitly allow edits.
  const baseUrlLocked =
    isEdit &&
    BUILTIN_PP_IDS.has(existing!.id) &&
    !(existing!.allow_base_url_edit ?? false);

  const [label, setLabel] = useState(existing?.label ?? "");
  const [baseUrl, setBaseUrl] = useState(existing?.base_url ?? "");
  const [model, setModel] = useState(existingModel ?? "");
  const [apiKey, setApiKey] = useState("");
  const [quotaLimit, setQuotaLimit] = useState(
    existing?.quota_limit != null ? String(existing.quota_limit) : "",
  );
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [fetching, setFetching] = useState(false);
  const [saving, setSaving] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  const canFetchModels = useMemo(
    () => isEdit && !!onFetchModels,
    [isEdit, onFetchModels],
  );

  const handleFetchModels = async () => {
    if (!existing || !onFetchModels) return;
    setFetching(true);
    try {
      setModelOptions(await onFetchModels(existing.id));
    } finally {
      setFetching(false);
    }
  };

  const handleSave = async () => {
    if (!label.trim()) {
      setValidationError(t("settings.postProcessing.pool.form.labelRequired"));
      return;
    }
    if (!baseUrl.trim()) {
      setValidationError(
        t("settings.postProcessing.pool.form.baseUrlRequired"),
      );
      return;
    }
    setValidationError(null);

    const parsedQuota = quotaLimit.trim() === "" ? null : Number(quotaLimit);
    const provider: PostProcessProvider = existing
      ? { ...existing, label: label.trim(), base_url: baseUrl.trim() }
      : {
          id: newId(),
          label: label.trim(),
          base_url: baseUrl.trim(),
          allow_base_url_edit: true,
          models_endpoint: "/models",
          supports_structured_output: false,
          enabled: true,
          quota_limit: null,
          quota_used_today: 0,
        };
    provider.quota_limit =
      parsedQuota != null && Number.isFinite(parsedQuota) && parsedQuota > 0
        ? Math.floor(parsedQuota)
        : null;

    const keyArg = apiKey.trim() === "" ? (isEdit ? null : "") : apiKey.trim();
    const modelArg = model.trim() === "" ? (isEdit ? null : "") : model.trim();

    setSaving(true);
    try {
      await onSave(provider, keyArg, modelArg);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="px-4 py-3 rounded-xl border border-accent/40 bg-paper-raised space-y-3">
      <h4 className="text-sm font-semibold">
        {isEdit
          ? t("settings.postProcessing.pool.form.editTitle")
          : t("settings.postProcessing.pool.form.addTitle")}
      </h4>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.form.label")}
          </span>
          <Input
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={t("settings.postProcessing.pool.form.labelPlaceholder")}
            variant="compact"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.form.quotaLimit")}
          </span>
          <Input
            type="number"
            min={0}
            value={quotaLimit}
            onChange={(e) => setQuotaLimit(e.target.value)}
            placeholder={t(
              "settings.postProcessing.pool.form.quotaLimitPlaceholder",
            )}
            variant="compact"
          />
        </label>
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-ink-soft">
          {t("settings.postProcessing.pool.form.baseUrl")}
        </span>
        <Input
          type="text"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder={t("settings.postProcessing.pool.form.baseUrlPlaceholder")}
          variant="compact"
          disabled={baseUrlLocked}
        />
      </label>

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-ink-soft">
          {t("settings.postProcessing.pool.form.model")}
        </span>
        <div className="flex items-center gap-2">
          <Input
            type="text"
            value={model}
            list={canFetchModels ? datalistId : undefined}
            onChange={(e) => setModel(e.target.value)}
            placeholder={t("settings.postProcessing.pool.form.modelPlaceholder")}
            variant="compact"
            className="flex-1"
          />
          {canFetchModels && (
            <button
              type="button"
              onClick={handleFetchModels}
              disabled={fetching}
              title={t("settings.postProcessing.pool.form.fetchModels")}
              className="shrink-0 p-2 rounded-lg border border-line text-ink-soft hover:border-accent hover:text-ink transition-colors cursor-pointer disabled:opacity-50"
            >
              <RefreshCcw
                className={`w-4 h-4 ${fetching ? "animate-spin" : ""}`}
              />
            </button>
          )}
        </div>
        {canFetchModels && (
          <datalist id={datalistId}>
            {modelOptions.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        )}
      </label>

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-ink-soft">
          {t("settings.postProcessing.pool.form.apiKey")}
        </span>
        <Input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={
            isEdit
              ? t("settings.postProcessing.pool.form.apiKeyKeepPlaceholder")
              : t("settings.postProcessing.pool.form.apiKeyPlaceholder")
          }
          variant="compact"
        />
      </label>

      <p className="text-xs text-ink-faint">
        {t("settings.postProcessing.pool.form.multiKeyTip")}
      </p>

      {validationError && (
        <p className="text-xs text-status-error">{validationError}</p>
      )}

      <div className="flex gap-2 pt-1">
        <Button
          onClick={handleSave}
          variant="primary"
          size="md"
          disabled={saving}
        >
          {t("settings.postProcessing.pool.form.save")}
        </Button>
        <Button onClick={onCancel} variant="secondary" size="md">
          {t("settings.postProcessing.pool.form.cancel")}
        </Button>
      </div>
    </div>
  );
};
