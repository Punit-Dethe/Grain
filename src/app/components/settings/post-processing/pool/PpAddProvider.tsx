import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { Dropdown, type DropdownOption } from "../../../ui/Dropdown";
import { Input } from "../../../ui/Input";
import { Button } from "../../../ui/Button";
import type { PostProcessProvider } from "@/bindings";

interface PpAddProviderProps {
  /** Built-in providers used as templates for the "available providers" picker. */
  templates: PostProcessProvider[];
  onAdd: (
    provider: PostProcessProvider,
    apiKey: string,
    model: string | null,
  ) => Promise<void>;
  /** Dismiss the form (Cancel, or after a successful add). */
  onClose?: () => void;
}

const newId = (): string =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? `pp_${crypto.randomUUID()}`
    : `pp_${Date.now()}_${Math.random().toString(36).slice(2)}`;

export const PpAddProvider: React.FC<PpAddProviderProps> = ({
  templates,
  onAdd,
  onClose,
}) => {
  const { t } = useTranslation();

  const options = useMemo<DropdownOption[]>(
    () => templates.map((tpl) => ({ value: tpl.id, label: tpl.label })),
    [templates],
  );

  const [templateId, setTemplateId] = useState<string>(
    templates[0]?.id ?? "",
  );
  const [name, setName] = useState("");
  const [customBaseUrl, setCustomBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const template = useMemo(
    () => templates.find((tpl) => tpl.id === templateId),
    [templates, templateId],
  );
  const isCustom = template?.id === "custom";

  const handleTemplateChange = (value: string) => {
    setTemplateId(value);
    setError(null);
  };

  const reset = () => {
    setName("");
    setCustomBaseUrl("");
    setApiKey("");
    setModel("");
  };

  const handleAdd = async () => {
    if (!template) return;
    if (!apiKey.trim()) {
      setError(t("settings.postProcessing.pool.add.keyRequired"));
      return;
    }
    const baseUrl = isCustom ? customBaseUrl.trim() : template.base_url;
    if (isCustom && !baseUrl) {
      setError(t("settings.postProcessing.pool.add.baseUrlRequired"));
      return;
    }
    setError(null);

    const provider: PostProcessProvider = {
      id: newId(),
      label: name.trim() || template.label,
      base_url: baseUrl,
      allow_base_url_edit: template.allow_base_url_edit ?? isCustom,
      models_endpoint: template.models_endpoint ?? "/models",
      supports_structured_output: template.supports_structured_output ?? false,
      enabled: true,
      quota_limit: null,
      quota_used_today: 0,
    };

    setSaving(true);
    try {
      await onAdd(provider, apiKey.trim(), model.trim() || null);
      reset();
      onClose?.();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="px-4 py-3 rounded-xl border border-accent/40 bg-paper-raised space-y-3">
      <h4 className="text-sm font-semibold">
        {t("settings.postProcessing.pool.addTitle")}
      </h4>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.add.provider")}
          </span>
          <Dropdown
            options={options}
            selectedValue={templateId}
            onSelect={handleTemplateChange}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.add.name")}
          </span>
          <Input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={template?.label ?? ""}
            variant="compact"
          />
        </label>
      </div>

      {isCustom && (
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.add.baseUrl")}
          </span>
          <Input
            type="text"
            value={customBaseUrl}
            onChange={(e) => setCustomBaseUrl(e.target.value)}
            placeholder={t("settings.postProcessing.pool.add.baseUrlPlaceholder")}
            variant="compact"
          />
        </label>
      )}

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.add.apiKey")}
          </span>
          <Input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={t("settings.postProcessing.pool.add.apiKeyPlaceholder")}
            variant="compact"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-soft">
            {t("settings.postProcessing.pool.add.model")}
          </span>
          <Input
            type="text"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder={t("settings.postProcessing.pool.add.modelPlaceholder")}
            variant="compact"
          />
        </label>
      </div>

      {error && <p className="text-xs text-status-error">{error}</p>}

      <div className="flex gap-2 pt-1">
        <Button
          onClick={handleAdd}
          variant="primary"
          size="md"
          disabled={saving}
          className="inline-flex items-center gap-1.5"
        >
          <Plus className="w-4 h-4" />
          {t("settings.postProcessing.pool.add.button")}
        </Button>
        {onClose && (
          <Button onClick={onClose} variant="secondary" size="md">
            {t("settings.postProcessing.pool.add.cancel")}
          </Button>
        )}
      </div>
    </div>
  );
};
