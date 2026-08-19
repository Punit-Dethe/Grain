import type { ExtensionCard, PromptLayerInfo } from "@/bindings";
import { promptLayerScope } from "./extensionRuntime";

/**
 * The slot an extension takes to speak for the surface instead of Grain.
 * Mirrors `grain_sdk::manifest::PROMPT_CONTEXT_SLOT`.
 */
export const PROMPT_CONTEXT_SLOT = "prompt.context";

export interface PromptStackRow {
  key: string;
  /** Who wrote it, in the user's terms. */
  source: "you" | "extension" | "grain";
  title: string;
  detail: string;
  /** Contributing to dictations right now. Inactive rows are still shown —
   *  the ladder is only understandable as a whole. */
  active: boolean;
  /** Verbatim text, for the rows that carry any. */
  layers: PromptLayerInfo[];
}

export interface PromptStackInput {
  contextAwarenessEnabled: boolean;
  /** How many app/site rules the user has written. */
  customProfileCount: number;
  /** The user's selected post-processing prompt, by name. */
  basePromptName: string | null;
  cards: ExtensionCard[];
}

/**
 * The authority ladder, populated with what is actually configured.
 *
 * Ordered by authority, highest first — the same order `compose_prompt` tells
 * the model, so the two cannot describe different hierarchies to two audiences.
 *
 * Deliberately NOT a live preview of the next dictation: the foreground app
 * while this is on screen is Grain's own settings window, so anything
 * "currently detected" would be a reading of the wrong surface. This answers
 * "what can shape my dictation, and in what order", which is the question
 * attribution actually has to answer.
 */
export function promptStackRows(input: PromptStackInput): PromptStackRow[] {
  const enabled = input.cards.filter((card) => card.enabled);
  const contextOwner = enabled.find((card) =>
    (card.slots ?? []).includes(PROMPT_CONTEXT_SLOT),
  );
  const contributors = enabled.filter(
    (card) => (card.prompt_layers ?? []).length > 0,
  );

  const rows: PromptStackRow[] = [
    {
      key: "spoken",
      source: "you",
      title: "What you say mid-recording",
      detail:
        "Click the pill while recording and dictate an instruction. It outranks everything below for that one transcript.",
      active: true,
      layers: [],
    },
    {
      key: "yours",
      source: "you",
      title: "Your prompt and your app rules",
      detail: input.customProfileCount
        ? `${input.basePromptName ?? "Your selected prompt"}, plus ${input.customProfileCount} rule${input.customProfileCount === 1 ? "" : "s"} you wrote for specific apps or sites.`
        : (input.basePromptName ?? "Your selected post-processing prompt") +
          ". Rules you write for a specific app or site rank here too.",
      active: true,
      layers: [],
    },
  ];

  for (const card of contributors) {
    rows.push({
      key: `ext:${card.id}`,
      source: "extension",
      title: card.name,
      detail: `${card.prompt_layers.length} instruction${card.prompt_layers.length === 1 ? "" : "s"} · ranks below anything you wrote`,
      active: true,
      layers: card.prompt_layers,
    });
  }

  rows.push(
    contextOwner
      ? {
          key: "context",
          source: "extension",
          title: "What Grain says about the app you're in",
          detail: `Replaced by ${contextOwner.name}. Grain no longer adds its own read of the surface.`,
          active: true,
          layers: [],
        }
      : {
          key: "context",
          source: "grain",
          title: "What Grain says about the app you're in",
          detail: input.contextAwarenessEnabled
            ? "Grain's own read of the surface — tone and vocabulary only, never structure."
            : "Off. Turn on context awareness above for Grain to adapt to the app you're typing in.",
          active: input.contextAwarenessEnabled,
          layers: [],
        },
  );

  return rows;
}

/** The one-line "when does this apply" label, re-exported so the panel has a
 *  single import. */
export { promptLayerScope };
