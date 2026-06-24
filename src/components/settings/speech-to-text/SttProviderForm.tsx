import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown, type DropdownOption } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { Button } from "../../ui/Button";
import type { SttProvider, SttProviderKind } from "@/bindings";

interface SttProviderFormProps {
  /** When editing, the existing provider; omit to add a new one. */
  existing?: SttProvider;
  onSave: (provider: SttProvider, apiKey: string | null) => Promise<void>;
  onCancel: () => void;
}

const DEFAULT_BASE_URL: Record<SttProviderKind, string> = {
  local: "",
  openai: "https://api.openai.com/v1",
  deepgram: "https://api.deepgram.com/v1",
  assemblyai: "https://api.assemblyai.com/v2",
};

const newId = (): string =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? `stt_${crypto.randomUUID()}`
    : `stt_${Date.now()}_${Math.random().toString(36).slice(2)}`;

export const SttProviderForm: React.FC<SttProviderFormProps> = ({
  existing,
  onSave,
  onCancel,
}) => {
  const { t } = useTranslation();
  const isEdit = !!existing;

  const [name, setName] = useState(existing?.name ?? "");
  const [kind, setKind] = useState<SttProviderKind>(
    (existing?.kind as SttProviderKind) ?? "openai",
  );
  const [baseUrl, setBaseUrl] = useState(
    existing?.base_url ?? DEFAULT_BASE_URL.openai,
  );
  const [model, setModel] = useState(existing?.model ?? "");
  const [apiKey, setApiKey] = useState("");
  const [quotaLimit, setQuotaLimit] = useState(
    existing?.quota_limit != null ? String(existing.quota_limit) : "",
  );
  const [saving, setSaving] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  const kindOptions = useMemo<DropdownOption[]>(
    () => [
      { value: "openai", label: t("settings.speechToText.form.kindOpenai") },
      {
        value: "deepgram",
        label: t("settings.speechToText.form.kindDeepgram"),
      },
      {
        value: "assemblyai",
        label: t("settings.speechToText.form.kindAssemblyai"),
      },
    ],
    [t],
  );

  const handleKindChange = (value: string) => {
    const next = value as SttProviderKind;
    setKind(next);
    // Only auto-fill the base URL when the field still holds a known default,
    // so we never clobber a URL the user has deliberately typed.
    const knownDefaults = Object.values(DEFAULT_BASE_URL);
    if (!baseUrl.trim() || knownDefaults.includes(baseUrl.trim())) {
      setBaseUrl(DEFAULT_BASE_URL[next]);
    }
  };

  const handleSave = async () => {
    if (!name.trim()) {
      setValidationError(t("settings.speechToText.form.nameRequired"));
      return;
    }
    if (!baseUrl.trim()) {
      setValidationError(t("settings.speechToText.form.baseUrlRequired"));
      return;
    }
    setValidationError(null);

    const parsedQuota = quotaLimit.trim() === "" ? null : Number(quotaLimit);
    const provider: SttProvider = {
      id: existing?.id ?? newId(),
      name: name.trim(),
      kind,
      base_url: baseUrl.trim(),
      model: model.trim(),
      // Keep the existing enabled state on edit; new entries join rotation by default.
      enabled: existing?.enabled ?? true,
      quota_limit:
        parsedQuota != null && Number.isFinite(parsedQuota) && parsedQuota > 0
          ? Math.floor(parsedQuota)
          : null,
      quota_used_today: existing?.quota_used_today ?? 0,
    };

    // On edit, an empty key means "keep the stored key" (pass null). On add, an
    // empty key is allowed too (the provider just won't be usable until set).
    const keyArg = apiKey.trim() === "" ? (isEdit ? null : "") : apiKey.trim();

    setSaving(true);
    try {
      await onSave(provider, keyArg);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="px-4 py-3 rounded-xl border border-accent/40 bg-paper-raised space-y-3">
      <h4 className="text-sm font-semibold">
        {isEdit
          ? t("settings.speechToText.form.editTitle")
          : t("settings.speechToText.form.addTitle")}
      </h4>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.speechToText.form.name")}
          </span>
          <Input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("settings.speechToText.form.namePlaceholder")}
            variant="compact"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.speechToText.form.kind")}
          </span>
          <Dropdown
            options={kindOptions}
            selectedValue={kind}
            onSelect={handleKindChange}
          />
        </label>
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-ink-soft">
          {t("settings.speechToText.form.baseUrl")}
        </span>
        <Input
          type="text"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder={t("settings.speechToText.form.baseUrlPlaceholder")}
          variant="compact"
        />
      </label>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.speechToText.form.model")}
          </span>
          <Input
            type="text"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder={t("settings.speechToText.form.modelPlaceholder")}
            variant="compact"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.speechToText.form.quotaLimit")}
          </span>
          <Input
            type="number"
            min={0}
            value={quotaLimit}
            onChange={(e) => setQuotaLimit(e.target.value)}
            placeholder={t("settings.speechToText.form.quotaLimitPlaceholder")}
            variant="compact"
          />
        </label>
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-ink-soft">
          {t("settings.speechToText.form.apiKey")}
        </span>
        <Input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={
            isEdit
              ? t("settings.speechToText.form.apiKeyKeepPlaceholder")
              : t("settings.speechToText.form.apiKeyPlaceholder")
          }
          variant="compact"
        />
      </label>

      <p className="text-xs text-ink-faint">
        {t("settings.speechToText.form.multiKeyTip")}
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
          {t("settings.speechToText.form.save")}
        </Button>
        <Button onClick={onCancel} variant="secondary" size="md">
          {t("settings.speechToText.form.cancel")}
        </Button>
      </div>
    </div>
  );
};
