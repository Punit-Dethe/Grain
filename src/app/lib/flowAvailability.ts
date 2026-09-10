import type { ModelInfo } from "@/bindings";

const REVIEWED_QUANTIZATIONS = new Set([
  "Q4_K_M",
  "Q5_K_M",
  "Q6_K",
  "Q8_0",
  "F16",
  "F32",
]);

export type FlowUnavailableReason =
  "model" | "translation" | "modelAndTranslation";

/** Mirrors `grain_core::capture::is_reviewed_flow_model`. Keep this exact: a
 * family-like custom filename is not a reviewed Flow artifact. */
export function isReviewedFlowModelId(modelId: string): boolean {
  const slash = modelId.lastIndexOf("/");
  if (slash < 0) return false;
  const repository = modelId.slice(0, slash);
  const filename = modelId.slice(slash + 1);
  const version =
    repository === "handy-computer/parakeet-tdt-0.6b-v2-gguf"
      ? "v2"
      : repository === "handy-computer/parakeet-tdt-0.6b-v3-gguf"
        ? "v3"
        : null;
  if (!version) return false;
  const prefix = `parakeet-tdt-0.6b-${version}-`;
  if (!filename.startsWith(prefix) || !filename.endsWith(".gguf")) return false;
  return REVIEWED_QUANTIZATIONS.has(
    filename.slice(prefix.length, -".gguf".length),
  );
}

export function getFlowAvailability(
  models: ModelInfo[],
  selectedModel: string,
  translateToEnglish: boolean,
): { available: true } | { available: false; reason: FlowUnavailableReason } {
  const selected = models.find((model) => model.id === selectedModel);
  const modelReady =
    Boolean(selected?.is_downloaded) && isReviewedFlowModelId(selectedModel);
  if (!modelReady && translateToEnglish) {
    return { available: false, reason: "modelAndTranslation" };
  }
  if (!modelReady) {
    return { available: false, reason: "model" };
  }
  if (translateToEnglish) return { available: false, reason: "translation" };
  return { available: true };
}
