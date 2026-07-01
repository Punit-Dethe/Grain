import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Alert } from "../../ui/Alert";
import type { SttProvider } from "@/bindings";
import { useSttPool } from "./useSttPool";
import { SttProviderRow } from "./SttProviderRow";
import { SttProviderForm } from "./SttProviderForm";
import { LocalModelSection } from "./LocalModelSection";
import { AsrModelSection } from "./AsrModelSection";
import { ProviderPool } from "../ProviderPool";
import { RollingWindow } from "../RollingWindow";
import { ModelUnloadTimeoutSetting } from "../ModelUnloadTimeout";

// [GRAIN] The unified Transcription tab. Top to bottom: the local model
// (collapsible picker), the local engine settings, then the cloud providers — one
// surface for the whole transcription pipeline. Smart rotation lives in the cloud
// header; turning it on grays out the local model (cloud handles transcription),
// and it's blocked (with an error) when no provider is configured.
export const SpeechToTextSettings: React.FC = () => {
  const { t } = useTranslation();
  const pool = useSttPool();
  const [togglingRotation, setTogglingRotation] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [rotationError, setRotationError] = useState(false);

  const { smartRotation, cloudProviders, providersWithKeys } = pool;

  // Matches the backend's rotation eligibility: a provider counts only when it's
  // enabled AND has a key. If none qualify while rotation is on, the router errors.
  const anyEligible = cloudProviders.some(
    (p) => (p.enabled ?? true) && providersWithKeys.has(p.id),
  );
  const showEmptyPoolWarning = smartRotation && !anyEligible;

  const handleToggleRotation = async (enabled: boolean) => {
    // Don't let rotation turn on with no providers — there'd be nothing to route
    // to. Block it and tell the user to configure one first.
    if (enabled && cloudProviders.length === 0) {
      setRotationError(true);
      return;
    }
    setRotationError(false);
    setTogglingRotation(true);
    try {
      await pool.setSmartRotation(enabled);
    } finally {
      setTogglingRotation(false);
    }
  };

  const handleSave = async (provider: SttProvider, apiKey: string | null) => {
    await pool.upsertProvider(provider, apiKey);
    setShowAddForm(false);
    setEditingId(null);
    setRotationError(false);
  };

  const closeForms = () => {
    setShowAddForm(false);
    setEditingId(null);
  };

  const isFormOpen = showAddForm || editingId !== null;

  if (pool.loading) {
    return (
      <div className="max-w-4xl w-full mx-auto">
        <div className="flex items-center justify-center py-16">
          <div className="w-8 h-8 border-2 border-accent border-t-transparent rounded-full animate-spin" />
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <div className="px-1">
        <h1 className="text-xl font-semibold mb-1">
          {t("settings.speechToText.title")}
        </h1>
        <p className="text-sm text-ink-soft">
          {t("settings.speechToText.description")}
        </p>
      </div>

      {/* 1) Local model — collapsible picker; grays out while cloud rotation is on. */}
      <LocalModelSection disabled={smartRotation} />

      {/* 1b) Streaming / Native-ASR model — identical collapsible picker against
          the separate ASR registry. Self-hides unless Experimental is enabled. */}
      <AsrModelSection />

      {/* 2) Engine — local-model behaviour: rolling buffer, unload. */}
      <SettingsGroup title={t("settings.speechToText.groups.engine")}>
        <RollingWindow descriptionMode="tooltip" grouped />
        <ModelUnloadTimeoutSetting descriptionMode="tooltip" grouped />
      </SettingsGroup>

      {/* 3) Cloud providers — header carries smart rotation (+ info) and add. */}
      <div className="space-y-2.5">
        {rotationError && (
          <Alert variant="error">
            {t("settings.speechToText.rotationNoProvider")}
          </Alert>
        )}
        {showEmptyPoolWarning && (
          <Alert variant="warning">
            {t("settings.speechToText.emptyPoolWarning")}
          </Alert>
        )}

        <ProviderPool
          title={t("settings.speechToText.groups.providers")}
          addLabel={t("settings.speechToText.addProvider")}
          onAdd={() => {
            setEditingId(null);
            setShowAddForm(true);
          }}
          addDisabled={isFormOpen}
          smartRotation={smartRotation}
          onToggleRotation={handleToggleRotation}
          togglingRotation={togglingRotation}
          rotationLabel={t("settings.speechToText.smartRotation.short")}
          rotationInfo={t("settings.speechToText.smartRotation.description")}
        >
          {showAddForm && (
            <div className="p-3">
              <SttProviderForm onSave={handleSave} onCancel={closeForms} />
            </div>
          )}

          {cloudProviders.length === 0 && !showAddForm ? (
            <div className="px-4 py-5 text-sm text-ink-soft text-center">
              {t("settings.speechToText.noProviders")}
            </div>
          ) : (
            cloudProviders.map((provider) =>
              editingId === provider.id ? (
                <div key={provider.id} className="p-3">
                  <SttProviderForm
                    existing={provider}
                    onSave={handleSave}
                    onCancel={closeForms}
                  />
                </div>
              ) : (
                <SttProviderRow
                  key={provider.id}
                  provider={provider}
                  hasKey={providersWithKeys.has(provider.id)}
                  inactive={!smartRotation}
                  onToggleRotate={pool.setProviderEnabled}
                  onEdit={(p) => {
                    setShowAddForm(false);
                    setEditingId(p.id);
                  }}
                  onRemove={pool.removeProvider}
                />
              ),
            )
          )}
        </ProviderPool>
      </div>
    </div>
  );
};
