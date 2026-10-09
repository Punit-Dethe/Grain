import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Alert } from "../../../ui/Alert";
import type { PostProcessProvider } from "@/bindings";
import { usePpPool, isBuiltinPpId } from "./usePpPool";
import { PpProviderRow } from "./PpProviderRow";
import { PpProviderForm } from "./PpProviderForm";
import { PpAddProvider } from "./PpAddProvider";
import { ProviderPool } from "../../ProviderPool";
import { reorderVisibleProviders } from "@/lib/providerOrder";

// [GRAIN] Saved provider order is the fallback priority.
export const PostProcessingPool: React.FC = () => {
  const { t } = useTranslation();
  const pool = usePpPool();
  const [togglingFallback, setTogglingFallback] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [fallbackError, setFallbackError] = useState(false);

  const {
    fallbackEnabled,
    providers,
    selectedProviderId,
    configuredProviderIds,
    models,
  } = pool;

  // Templates for the add picker = the seeded built-ins. The list only shows
  // providers the backend considers configured, including keyless local models.
  // "Custom" pinned last. The backend appends providers it has newly seeded to
  // the END of an existing user's stored list, so a provider added after they
  // first ran Grain (Gemini) would otherwise sit below the catch-all.
  const templates = providers
    .filter((p) => isBuiltinPpId(p.id))
    .sort((a, b) => Number(a.id === "custom") - Number(b.id === "custom"));
  const listRef = useRef<HTMLDivElement>(null);
  const pendingRef = useRef(false);
  const keyboardFocusRef = useRef<string | null>(null);
  const dragRef = useRef<{
    id: string;
    pointerId: number;
    capture: HTMLDivElement;
    all: string[];
    visible: string[];
    centers: number[];
    top: number;
    position: number;
    order: string[];
  } | null>(null);
  const [previewOrder, setPreviewOrder] = useState<string[] | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [savingOrder, setSavingOrder] = useState(false);
  const [orderError, setOrderError] = useState(false);
  const [orderStatus, setOrderStatus] = useState("");
  const configuredIds = providers
    .filter((p) => configuredProviderIds.has(p.id))
    .map((p) => p.id);
  const configured = (
    previewOrder
      ? previewOrder
          .map((id) => providers.find((p) => p.id === id))
          .filter((p): p is PostProcessProvider => !!p)
      : providers
  ).filter((p) => configuredProviderIds.has(p.id));
  const isFormOpen = showAddForm || editingId !== null;

  const cancelDrag = () => {
    const drag = dragRef.current;
    dragRef.current = null;
    if (drag?.capture.hasPointerCapture(drag.pointerId))
      drag.capture.releasePointerCapture(drag.pointerId);
    setDraggingId(null);
    setPreviewOrder(null);
  };

  useEffect(() => {
    // A concurrent settings update invalidates a gesture's snapshot.
    cancelDrag();
    return () => {
      const drag = dragRef.current;
      dragRef.current = null;
      if (drag?.capture.hasPointerCapture(drag.pointerId))
        drag.capture.releasePointerCapture(drag.pointerId);
    };
  }, [providers, fallbackEnabled]);

  useLayoutEffect(() => {
    const id = keyboardFocusRef.current;
    if (!id || savingOrder) return;
    keyboardFocusRef.current = null;
    const list = listRef.current;
    if (!list) return;
    // A moved DOM subtree can lose focus. Restore it after rows are enabled,
    // unless the user has deliberately moved focus elsewhere during the save.
    const active = list.ownerDocument.activeElement as HTMLElement | null;
    if (
      active &&
      active !== list.ownerDocument.body &&
      active.dataset.providerGrip !== id
    )
      return;
    Array.from(list.querySelectorAll<HTMLButtonElement>("[data-provider-grip]"))
      .find((button) => button.dataset.providerGrip === id && !button.disabled)
      ?.focus({ preventScroll: true });
  }, [savingOrder, providers, previewOrder, fallbackEnabled]);

  const saveOrder = async (
    order: string[],
    id: string,
    restoreFocus = false,
  ) => {
    if (
      pendingRef.current ||
      order.every((value, i) => value === providers[i]?.id)
    ) {
      setPreviewOrder(null);
      return;
    }
    pendingRef.current = true;
    if (restoreFocus) keyboardFocusRef.current = id;
    setSavingOrder(true);
    setOrderError(false);
    setPreviewOrder(order);
    try {
      await pool.reorderProviders(order);
      const visibleOrder = order.filter((value) =>
        configuredProviderIds.has(value),
      );
      setOrderStatus(
        t("settings.postProcessing.pool.orderStatus", {
          name: providers.find((p) => p.id === id)?.label,
          position: visibleOrder.indexOf(id) + 1,
        }),
      );
    } catch {
      setOrderError(true);
      // Reload after a rejected stale order, retaining canonical backend state.
      await pool.reload().catch(() => undefined);
    } finally {
      pendingRef.current = false;
      setSavingOrder(false);
      setPreviewOrder(null);
    }
  };

  const startDrag = (
    event: React.PointerEvent<HTMLButtonElement>,
    id: string,
  ) => {
    if (
      event.button !== 0 ||
      !fallbackEnabled ||
      isFormOpen ||
      pendingRef.current ||
      togglingFallback ||
      dragRef.current
    )
      return;
    const list = listRef.current;
    if (!list) return;
    event.preventDefault();
    event.currentTarget.focus();
    list.setPointerCapture(event.pointerId);
    const all = providers.map((p) => p.id);
    dragRef.current = {
      id,
      pointerId: event.pointerId,
      capture: list,
      all,
      visible: configuredIds,
      centers: Array.from(
        list.querySelectorAll<HTMLElement>("[data-provider-row]"),
      )
        .filter((row) => row.dataset.providerRow !== id)
        .map((row) => {
          const rect = row.getBoundingClientRect();
          return rect.top + rect.height / 2;
        }),
      top: list.getBoundingClientRect().top,
      position: configuredIds.indexOf(id),
      order: all,
    };
    setDraggingId(id);
    setOrderError(false);
  };

  const moveDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || event.pointerId !== drag.pointerId || !listRef.current) return;
    // Original row centers avoid oscillation as React moves rows. Adjust for scrolling.
    const y =
      event.clientY - (listRef.current.getBoundingClientRect().top - drag.top);
    const position = drag.centers.filter((center) => center < y).length;
    if (position === drag.position) return;
    drag.position = position;
    drag.order = reorderVisibleProviders(
      drag.all,
      drag.visible,
      drag.id,
      position,
    );
    setPreviewOrder(drag.order);
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || event.pointerId !== drag.pointerId) return;
    dragRef.current = null;
    if (drag.capture.hasPointerCapture(drag.pointerId))
      drag.capture.releasePointerCapture(drag.pointerId);
    setDraggingId(null);
    void saveOrder(drag.order, drag.id);
  };

  const moveWithKeyboard = (id: string, direction: number) => {
    if (
      !fallbackEnabled ||
      isFormOpen ||
      pendingRef.current ||
      dragRef.current ||
      togglingFallback
    )
      return;
    void saveOrder(
      reorderVisibleProviders(
        providers.map((p) => p.id),
        configuredIds,
        id,
        configuredIds.indexOf(id) + direction,
      ),
      id,
      true,
    );
  };

  const anyEnabledWithKey = configured.some((p) => p.enabled ?? true);
  const showEmptyPoolWarning = fallbackEnabled && !anyEnabledWithKey;

  const handleToggleFallback = async (enabled: boolean) => {
    // Fallback needs at least one configured provider.
    if (enabled && configured.length === 0) {
      setFallbackError(true);
      return;
    }
    setFallbackError(false);
    setTogglingFallback(true);
    try {
      await pool.setFallbackEnabled(enabled);
    } catch {
      // The store exposes the command error below.
    } finally {
      setTogglingFallback(false);
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
    setFallbackError(false);
  };

  const handleEditSave = async (
    provider: PostProcessProvider,
    apiKey: string | null,
    model: string | null,
  ) => {
    await pool.upsertProvider(provider, apiKey, model);
    setEditingId(null);
  };

  if (pool.loading) {
    return (
      <div className="flex items-center justify-center py-10">
        <div className="w-6 h-6 border-2 border-accent border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <div
      ref={listRef}
      className="space-y-2.5"
      onPointerMove={moveDrag}
      onPointerUp={endDrag}
      onPointerCancel={(event) => {
        if (dragRef.current?.pointerId === event.pointerId) cancelDrag();
      }}
      onLostPointerCapture={(event) => {
        if (dragRef.current?.pointerId === event.pointerId) cancelDrag();
      }}
    >
      <p className="sr-only" aria-live="polite">
        {orderStatus}
      </p>
      {orderError && (
        <Alert variant="error">
          {t("settings.postProcessing.pool.saveOrderError")}
        </Alert>
      )}
      {pool.error && !orderError && <Alert variant="error">{pool.error}</Alert>}
      {fallbackError && (
        <Alert variant="error">
          {t("settings.postProcessing.pool.fallbackNoProvider")}
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
        addDisabled={
          isFormOpen || savingOrder || draggingId !== null || togglingFallback
        }
        fallbackEnabled={fallbackEnabled}
        onToggleFallback={handleToggleFallback}
        togglingFallback={togglingFallback}
        fallbackDisabled={savingOrder || draggingId !== null || isFormOpen}
        fallbackLabel={t("settings.postProcessing.pool.fallback.label")}
        fallbackInfo={t("settings.postProcessing.pool.fallback.description")}
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
          configured.map((provider, position) =>
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
                fallbackEnabled={fallbackEnabled}
                onToggleFallbackProvider={pool.setProviderEnabled}
                onSetActive={pool.setActiveProvider}
                onEdit={(p) => {
                  setShowAddForm(false);
                  setEditingId(p.id);
                }}
                onRemove={pool.removeProvider}
                position={position}
                reorderDisabled={savingOrder || isFormOpen || togglingFallback}
                dragging={draggingId === provider.id}
                onDragStart={startDrag}
                onMove={moveWithKeyboard}
              />
            ),
          )
        )}
      </ProviderPool>
    </div>
  );
};
