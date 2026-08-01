import type {
  ExtensionCard,
  ExtensionSettingsSection,
  Result,
  StoreEntry,
} from "@/bindings";

const NEVER_TOGGLED = "18446744073709551615";

const CAPABILITY_LABELS: Record<string, string> = {
  "events:sessions": "See when recording starts and stops",
  "events:transcripts": "Read what you dictate",
  "events:audio-levels": "See live microphone levels",
  "transform:transcript": "Rewrite your text before it is pasted",
  "session:start": "Start a recording session itself",
  storage: "Store its own data on this device",
  settings: "Save its own settings",
  llm: "Send text and images to your configured AI provider",
  embed: "Turn text into embeddings",
  notes: "Read and change all your Grain Space notes",
  "capture:selection": "Read your currently selected text",
  "capture:app": "See which app you're currently using",
  "capture:screen-text": "Read all the text on the window you're using",
  "capture:screen-image": "Take a screenshot of the window you are using",
  "open:url": "Open web links in your browser",
  "open:app": "Launch apps you choose",
};

const SLOT_LABELS: Record<string, string> = {
  "overlay.recording": "the recording overlay",
  "overlay.pointer": "the pointer overlay",
  "pill.theme": "the pill's look",
  "agent.reply-surface": "the Agent's reply panel",
  "output.destination": "where your text is sent",
};

export interface SlotConflict {
  slot: string;
  currentOccupant: string;
}

export type ExtensionDestination =
  | { kind: "tools"; section: "snippets" | "context" | "agent" }
  | {
      kind: "settings";
      section: "post-processing" | "speech-to-text";
    }
  | { kind: "notes-settings" }
  | { kind: "extension-settings"; extensionId: string }
  | { kind: "preview" };

export type ToolSection = "dictionary" | "snippets" | "context" | "agent";

export function unwrapResult<T, E>(result: Result<T, E>): T {
  if (result.status === "error") throw new Error(String(result.error));
  return result.data;
}

export function capabilityLabel(capability: string): string {
  return capability.startsWith("net:")
    ? `Send data to ${capability.slice("net:".length)}`
    : (CAPABILITY_LABELS[capability] ?? capability);
}

export function slotLabel(slot: string): string {
  return (
    SLOT_LABELS[slot] ??
    (slot.startsWith("overrides:")
      ? `the \u201c${slot.slice("overrides:".length)}\u201d setting`
      : slot)
  );
}

export function parseNeedsPermissions(error: unknown): string[] | null {
  try {
    const parsed = JSON.parse(String(error)) as { needsPermissions?: unknown };
    return Array.isArray(parsed.needsPermissions)
      ? (parsed.needsPermissions as string[])
      : null;
  } catch {
    return null;
  }
}

export function parseSlotConflict(error: unknown): SlotConflict | null {
  try {
    const parsed = JSON.parse(String(error)) as {
      slotConflict?: SlotConflict;
    };
    return parsed.slotConflict?.slot ? parsed.slotConflict : null;
  } catch {
    return null;
  }
}

export function sortExtensionCards<
  T extends Pick<ExtensionCard, "enabled" | "toggle_seq" | "name">,
>(cards: T[]): T[] {
  const sequence = (card: T) =>
    card.toggle_seq === NEVER_TOGGLED
      ? Number.MAX_SAFE_INTEGER
      : Number(card.toggle_seq);
  return [...cards].sort((left, right) => {
    if (left.enabled !== right.enabled) return left.enabled ? -1 : 1;
    const order = sequence(left) - sequence(right);
    return order !== 0 ? order : left.name.localeCompare(right.name);
  });
}

/**
 * Host surfaces an extension can take over, mapped to the place in the app
 * where that surface is actually configured.
 *
 * These are Grain's three extension shapes:
 *  - **full-page** — it has settings of its own, so it gets a page
 *    (`extension-settings`);
 *  - **in-place** — it has no page; it changes a control that already exists,
 *    like the Agent's Look picker. This map is what routes those;
 *  - **anchored** — it contributes rows into an existing section, routed by
 *    the anchors below.
 * Without this map an in-place extension dead-ended on a preview, leaving the
 * user to hunt for the control it changed.
 */
const SLOT_DESTINATIONS: Record<string, ExtensionDestination> = {
  "agent.reply-surface": { kind: "tools", section: "agent" },
};

export function extensionDestination(
  card: Pick<ExtensionCard, "id" | "has_detail"> & { slots?: string[] },
  sections: ExtensionSettingsSection[],
): ExtensionDestination {
  for (const slot of card.slots ?? []) {
    const destination = SLOT_DESTINATIONS[slot];
    if (destination) return destination;
  }

  const section = sections.find((candidate) => candidate.id === card.id);
  const anchors = new Set(
    (section?.rows ?? []).map((row) => row.anchor).filter(Boolean),
  );

  if (anchors.has("snippets.after")) {
    return { kind: "tools", section: "snippets" };
  }
  if (anchors.has("context.after")) {
    return { kind: "tools", section: "context" };
  }
  if (anchors.has("agent.after")) {
    return { kind: "tools", section: "agent" };
  }
  if (anchors.has("dictation.pipeline.after")) {
    return { kind: "settings", section: "post-processing" };
  }
  if (anchors.has("models.after")) {
    return { kind: "settings", section: "speech-to-text" };
  }
  if (anchors.has("grainspace.after")) return { kind: "notes-settings" };

  const hasOwnRows = (section?.rows ?? []).some(
    (row) =>
      !row.anchor ||
      ![
        "snippets.after",
        "context.after",
        "agent.after",
        "dictation.pipeline.after",
        "models.after",
        "grainspace.after",
      ].includes(row.anchor),
  );
  if (card.has_detail || hasOwnRows) {
    return { kind: "extension-settings", extensionId: card.id };
  }
  return { kind: "preview" };
}

const TOOL_TERMS: Record<ToolSection, readonly string[]> = {
  dictionary: ["dictionary", "vocabulary", "terminology", "word", "spelling"],
  snippets: ["snippet", "template", "variable", "expansion", "insert"],
  context: ["context", "application", "nearby", "cursor", "selection"],
  agent: ["agent", "prompt", "assistant", "rewrite", "action"],
};

export function recommendationScore(
  entry: StoreEntry,
  tool: ToolSection,
): number {
  const terms = TOOL_TERMS[tool];
  const categories = entry.categories.map((category) => category.toLowerCase());
  const text = `${entry.name} ${entry.id} ${entry.description}`.toLowerCase();
  let score = categories.includes(tool) ? 12 : 0;
  if (categories.includes("tools")) score += 3;
  for (const term of terms) {
    if (categories.includes(term)) score += 8;
    if (text.includes(term)) score += 2;
  }
  return score;
}

export function matchToolRecommendations(
  entries: StoreEntry[],
  tool: ToolSection,
  limit = 3,
): StoreEntry[] {
  return entries
    .map((entry) => ({ entry, score: recommendationScore(entry, tool) }))
    .filter(({ score }) => score > 0)
    .sort(
      (left, right) =>
        right.score - left.score ||
        right.entry.installs - left.entry.installs ||
        left.entry.name.localeCompare(right.entry.name),
    )
    .slice(0, limit)
    .map(({ entry }) => entry);
}

export function filterExtensions<
  T extends { id: string; name: string; description: string },
>(entries: T[], query: string): T[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) =>
    `${entry.name} ${entry.id} ${entry.description}`
      .toLocaleLowerCase()
      .includes(needle),
  );
}

export function nextMediaIndex(
  current: number,
  count: number,
  direction: -1 | 1,
): number {
  if (count <= 1) return 0;
  return (current + direction + count) % count;
}
