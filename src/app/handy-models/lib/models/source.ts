import type { ModelInfo } from "@/bindings";

// Handy's catalogue join. Grain does not carry its ONNX migration UI.
export const repoIdOf = (model: ModelInfo): string | null =>
  typeof model.source === "object" && "HuggingFace" in model.source
    ? model.source.HuggingFace.repo_id
    : null;
