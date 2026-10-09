import type { ModelInfo } from "@/bindings";

interface ModelIndicatorState {
  loading: boolean;
  currentModel: string;
  loadedModelId: string | null;
  models: readonly Pick<ModelInfo, "id" | "name">[];
  isModelLoaded: boolean;
}

/** Resident and last resident share one identity; unloading changes only the
 * status. Before the first load, use the selected Standard model as a fallback. */
export function getModelIndicatorStatus(state: ModelIndicatorState) {
  const activeId = state.loadedModelId || state.currentModel;
  if (state.loading && !activeId)
    return { title: "Checking model", subtitle: "Checking" };
  const name =
    state.models.find((model) => model.id === activeId)?.name ?? activeId;
  if (!name) return { title: "No model", subtitle: "Not loaded" };
  return {
    title: name,
    subtitle: `${state.isModelLoaded ? "Loaded" : "Unloaded"} · Local`,
  };
}
