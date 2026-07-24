import React from "react";
import { ActionsSection } from "./ActionsSection";

/** [GRAIN] Native custom cards (SPEC §4.1 Level 3, first-party half).
 *
 * A custom card is the escape hatch for settings UI the declarative controls
 * (toggle/dropdown/list/…) cannot express. It renders in the extension page,
 * below any declarative settings.
 *
 * There are two backings for the same slot:
 *   · FIRST-PARTY (this file) — a Grain React component, rendered inline. No
 *     iframe, no bridge: it is Grain's own trusted code, so it pays zero
 *     sandbox overhead ([[extensions-must-feel-native]]).
 *   · THIRD-PARTY — a `settingsPanel` declared in the manifest, rendered in a
 *     sandboxed iframe (the surface bridge). Untrusted code never runs inline.
 *
 * Keyed by extension id. An id absent here simply has no native card. */
export const NATIVE_CARDS: Record<string, React.FC> = {
  // Voice Actions: the Actions editor, exactly as it appeared under Snippets —
  // now this built-in's own custom card, the first one on the platform.
  "grain.voice-actions": ActionsSection,
};
