import type { OnboardingTestMode } from "@/bindings";

export type PreviewPhase = "ready" | "recording" | "finishing" | "complete";
export interface PreviewFrame {
  at: number;
  phase: PreviewPhase;
  captured: number;
  processed: number;
  transcript: string;
}
export const emptyPreview = (): PreviewFrame => ({
  at: 0,
  phase: "ready",
  captured: 0,
  processed: 0,
  transcript: "",
});

/** Illustrative steps, never performance measurements or real recordings. */
export function modePreviewFrames(
  mode: OnboardingTestMode,
  phrase: string,
  reducedMotion = false,
): PreviewFrame[] {
  if (reducedMotion) {
    return [
      {
        at: 0,
        phase: "recording",
        captured: 0.65,
        processed: mode === "standard" ? 0 : 0.45,
        transcript: mode === "streaming" ? phrase : "",
      },
      {
        at: 0,
        phase: "finishing",
        captured: 1,
        processed: mode === "standard" ? 0.5 : 0.85,
        transcript: mode === "streaming" ? phrase : "",
      },
      {
        at: 0,
        phase: "complete",
        captured: 1,
        processed: 1,
        transcript: phrase,
      },
    ];
  }
  if (mode === "streaming") {
    const words = phrase.trim().split(/\s+/).filter(Boolean);
    return [
      { at: 0, phase: "recording", captured: 0, processed: 0, transcript: "" },
      ...words.map((_, index) => ({
        at: (index + 1) * 300,
        phase: "recording" as const,
        captured: (index + 1) / words.length,
        processed: (index + 1) / words.length,
        transcript: words.slice(0, index + 1).join(" "),
      })),
      {
        at: (words.length + 1) * 300,
        phase: "complete",
        captured: 1,
        processed: 1,
        transcript: phrase,
      },
    ];
  }
  return [
    { at: 0, phase: "recording", captured: 0, processed: 0, transcript: "" },
    {
      at: 700,
      phase: "recording",
      captured: 0.3,
      processed: mode === "flow" ? 0.12 : 0,
      transcript: "",
    },
    {
      at: 1400,
      phase: "recording",
      captured: 0.65,
      processed: mode === "flow" ? 0.4 : 0,
      transcript: "",
    },
    {
      at: 2100,
      phase: "finishing",
      captured: 1,
      processed: mode === "flow" ? 0.8 : 0,
      transcript: "",
    },
    {
      at: 2600,
      phase: "finishing",
      captured: 1,
      processed: mode === "flow" ? 0.95 : 0.5,
      transcript: "",
    },
    {
      at: 3100,
      phase: "complete",
      captured: 1,
      processed: 1,
      transcript: phrase,
    },
  ];
}
