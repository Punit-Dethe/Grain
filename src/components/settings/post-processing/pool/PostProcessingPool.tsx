import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Alert } from "../../../ui/Alert";
import type { PostProcessProvider } from "@/bindings";
import { usePpPool, BUILTIN_PP_IDS } from "./usePpPool";
import { PpProviderRow } from "./PpProviderRow";
import { PpProviderForm } from "./PpProviderForm";
import { PpAddProvider } from "./PpAddProvider";
import { ProviderPool } from "../../ProviderPool";

// [GRAIN] The Processing (LLM) provider pool — the SAME interface as the
// Transcription cloud pool: "Add provider" on the left, smart rotation on the
// right, the configured providers listed below (edit / delete inline). The old
// always-on add form and separate routing group are gone. Rotation here has no
// local-model side effect; it's only blocked when no provider is configured.
export const PostProcessingPool: React.FC = () => {
  const { t } = useTranslation();
  const pool = usePpPool();
  const [togglingRotation, setTogglingRotation] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [rotationError, setRotationError] = useState(false);

  const {
    smartRotation,
    providers,
    selectedProviderId,
    providersWithKeys,
    models,
  } = pool;

  // Templates for the add picker = the seeded built-ins. The list only shows
  // providers the user has actually configured (have a key).
  const templates = providers.filter((p) => BUILTIN_PP_IDS.has(p.id));
  const configured = providers.filter((p) => providersWithKeys.has(p.id));

  const anyEnabledWithKey = configured.some((p) => p.enabled ?? true);
  const showEmptyPoolWarning = smartRotation && !anyEnabledWithKey;

  const handleToggleRotation = async (enabled: boolean) => {
    // Rotation needs at least one configured provider to rotate across — block it
    // and ask the user to add one first.
    if (enabled && configured.length === 0) {
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

  const handleAdd = async (
    provider: PostProcessProvider,
    apiKey: string,
    model: string | null,
  ) => {
    await pool.upsertProvider(provider, apiKey, model);
    // If nothing valid is the single-active provider yet, make this one active.
    const hadActiveKeyed = configured.some((p) => p.id === selectedProviderId);
    if (!hadActiveKeyed) await pool.setActiveProvider(provider.id);
    setRotationError(false);
  };

  const handleEditSave = async (
    provider: PostProcessProvider,
    apiKey: string | null,
    model: string | null,
  ) => {
    await pool.upsertProvider(provider, apiKey, model);
    setEditingId(null);
  };

  const isFormOpen = showAddForm || editingId !== null;

  if (pool.loading) {
    return (
      <div className="flex items-center justify-center py-10">
        <div className="w-6 h-6 border-2 border-accent border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-2.5">
      {rotationError && (
        <Alert variant="error">
          {t("settings.postProcessing.pool.rotationNoProvider")}
        </Alert>
      )}
      {showEmptyPoolWarning && (
        <Alert variant="warning">
          {t("settings.postProcessing.pool.emptyPoolWarning")}
        </Alert>
      )}

      <ProviderPool
        title={t("settings.postProcessing.pool.providersTitle")}
        addLabel={t("settings.postProcessing.pool.addProvider")}
        onAdd={() => {
          setEditingId(null);
          setShowAddForm(true);
        }}
        addDisabled={isFormOpen}
        smartRotation={smartRotation}
        onToggleRotation={handleToggleRotation}
        togglingRotation={togglingRotation}
        rotationLabel={t("settings.postProcessing.pool.smartRotation.label")}
        rotationInfo={t("settings.postProcessing.pool.smartRotation.description")}
      >
        {showAddForm && (
          <div className="p-3">
            <PpAddProvider
              templates={templates}
              onAdd={handleAdd}
              onClose={() => setShowAddForm(false)}
            />
          </div>
        )}

        {configured.length === 0 && !showAddForm ? (
          <div className="px-4 py-5 text-sm text-ink-soft text-center">
            {t("settings.postProcessing.pool.noConfigured")}
          </div>
        ) : (
          configured.map((provider) =>
            editingId === provider.id ? (
              <div key={provider.id} className="p-3">
                <PpProviderForm
                  existing={provider}
                  existingModel={models[provider.id] ?? ""}
                  onSave={handleEditSave}
                  onFetchModels={pool.fetchModels}
                  onCancel={() => setEditingId(null)}
                />
              </div>
            ) : (
              <PpProviderRow
                key={provider.id}
                provider={provider}
                model={models[provider.id] ?? ""}
                isActive={provider.id === selectedProviderId}
                smartRotation={smartRotation}
                onToggleRotate={pool.setProviderEnabled}
                onSetActive={pool.setActiveProvider}
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
  );
};
