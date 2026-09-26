import type {
  ActionInfo,
  ExtensionCard,
  ExtensionSettingsSection,
  PromptLayerInfo,
  Result,
  StoreEntry,
} from "@/bindings";

const NEVER_TOGGLED = "18446744073709551615";

const CAPABILITY_LABELS: Record<string, string> = {
  "events:sessions": "See when recording starts and stops",
  "events:transcripts": "Read what you dictate",
  "events:audio-levels": "See live microphone levels",
  resident: "Keep its worker running while Grain is open",
  "transform:transcript": "Rewrite your text before it is pasted",
  "session:start": "Start a recording session itself",
  storage: "Store its own data on this device",
  settings: "Save its own settings",
  llm: "Send text and images to your configured AI provider",
  embed: "Turn text into embeddings",
  "capture:selection": "Read your currently selected text",
  "capture:app": "See which app you're currently using",
  "capture:screen-text": "Read all the text on the window you're using",
  "capture:screen-image": "Take a screenshot of the window you are using",
  "open:url": "Open web links in your browser",
  "open:app": "Launch apps you choose",
};

const SLOT_LABELS: Record<string, string> = {
  "output.destination": "where your text is sent",
  "prompt.main": "Grain's main dictation instruction",
  "prompt.context": "what Grain tells the AI about the app you are typing in",
};

export interface SlotConflict {
  slot: string;
  currentOccupant: string;
}

export type ExtensionDestination =
  | { kind: "tools"; section: "snippets" | "context" }
  | { kind: "agent" }
  | {
      kind: "settings";
      section: "post-processing" | "speech-to-text";
    }
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
  return SLOT_LABELS[slot] ?? slot;
}

export interface ApprovalRequest {
  permissions: string[];
  promptLayers: PromptLayerInfo[];
  actions: ActionInfo[];
  authentication: AuthenticationApproval | null;
  recommendation: boolean;
}

export interface AuthenticationApproval {
  provider_name: string;
  scopes: string[];
  api_hosts: string[];
  authorization_host: string;
  token_host: string;
}

/**
 * The single structured error an enable can be refused with.
 *
 * Capabilities, prompt layers and actions arrive together and are shown in one
 * sheet: two sheets in a row for one enable is how a user learns to click
 * Approve without reading, which defeats the point of asking.
 */
export function parseApprovalRequest(error: unknown): ApprovalRequest | null {
  try {
    const parsed = JSON.parse(String(error)) as {
      needsPermissions?: unknown;
      needsPromptLayers?: unknown;
      needsActions?: unknown;
      needsAuthentication?: unknown;
      needsRecommendation?: unknown;
    };
    const permissions = Array.isArray(parsed.needsPermissions)
      ? (parsed.needsPermissions as string[])
      : [];
    const promptLayers = Array.isArray(parsed.needsPromptLayers)
      ? (parsed.needsPromptLayers as PromptLayerInfo[])
      : [];
    const actions = Array.isArray(parsed.needsActions)
      ? (parsed.needsActions as ActionInfo[])
      : [];
    const rawAuthentication = parsed.needsAuthentication;
    const authentication =
      rawAuthentication &&
      typeof rawAuthentication === "object" &&
      "provider_name" in rawAuthentication &&
      typeof rawAuthentication.provider_name === "string" &&
      "authorization_host" in rawAuthentication &&
      typeof rawAuthentication.authorization_host === "string" &&
      "token_host" in rawAuthentication &&
      typeof rawAuthentication.token_host === "string" &&
      "scopes" in rawAuthentication &&
      Array.isArray(rawAuthentication.scopes) &&
      rawAuthentication.scopes.every((scope) => typeof scope === "string") &&
      "api_hosts" in rawAuthentication &&
      Array.isArray(rawAuthentication.api_hosts) &&
      rawAuthentication.api_hosts.every((host) => typeof host === "string")
        ? (rawAuthentication as AuthenticationApproval)
        : null;
    // A malformed account request must never be approved under a sheet that
    // only happens to show its other permissions.
    if (rawAuthentication != null && !authentication) return null;
    const recommendation = parsed.needsRecommendation === true;
    if (
      !permissions.length &&
      !promptLayers.length &&
      !actions.length &&
      !authentication &&
      !recommendation
    )
      return null;
    return {
      permissions,
      promptLayers,
      actions,
      authentication,
      recommendation,
    };
  } catch {
    return null;
  }
}

/** Plain-language description of when a contributed layer applies. */
export function promptLayerScope(layer: PromptLayerInfo): string {
  const role =
    layer.target === "main"
      ? "Replaces Main prompt"
      : layer.target === "context"
        ? "Replaces Context provider"
        : "Adds a rule";
  if (layer.everywhere) return `${role} · Every dictation`;
  const parts: string[] = [];
  if (layer.website.length) parts.push(layer.website.join(", "));
  if (layer.app.length) parts.push(layer.app.join(", "));
  if (layer.category.length) parts.push(layer.category.join(", "));
  return parts.length
    ? `${role} · In ${parts.join(" · ")}`
    : `${role} · Every dictation`;
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
 * Where each host-owned settings contribution is configured.
 *
 * A destination id is what an extension *declares* it extends — a behavior slot
 * ([`KNOWN_SLOTS`]), an anchor its settings rows attach to ([`ANCHORS`]), or the
 * surface its payload feeds. This one table turns that declaration into a place
 * in the app, and it answers both halves of the same question:
 *
 *  - an **installed** card opens the surface it changes, and
 *  - a **store** card is recommended beside that surface.
 *
 * Those used to be two mechanisms. Routing read anchors; recommendations scored
 * keywords against the extension's prose — which put a prompt pack under Agent
 * because its description says "prompt", and put App Modes nowhere because it
 * never says "context", though its manifest anchors it to `context.after`. A
 * new extension is placed correctly by declaring a surface, and by nothing else.
 *
 * Extensions can take behavior slots, contribute rows at a host anchor, or get
 * a normal Grain-rendered settings page of their own.
 */
const SURFACE_DESTINATIONS: Record<string, ExtensionDestination> = {
  "agent.after": { kind: "agent" },
  "snippets.after": { kind: "tools", section: "snippets" },
  "context.after": { kind: "tools", section: "context" },
  "dictation.pipeline.after": { kind: "settings", section: "post-processing" },
  "dictation.prompts": { kind: "settings", section: "post-processing" },
  "models.after": { kind: "settings", section: "speech-to-text" },
};

const ANCHOR_SURFACES = new Set(
  Object.keys(SURFACE_DESTINATIONS).filter((surface) =>
    surface.endsWith(".after"),
  ),
);

/** Every surface an installed extension extends: slots it claims, plus the
 *  anchors its contributed settings rows attach to. */
function installedSurfaces(
  card: Pick<ExtensionCard, "id"> & { slots?: string[] },
  sections: ExtensionSettingsSection[],
): string[] {
  const section = sections.find((candidate) => candidate.id === card.id);
  const anchors = (section?.rows ?? [])
    .map((row) => row.anchor)
    .filter((anchor): anchor is string => Boolean(anchor));
  return [...(card.slots ?? []), ...anchors];
}

export function destinationForSurfaces(
  surfaces: readonly string[],
): ExtensionDestination | null {
  for (const surface of surfaces) {
    const destination = SURFACE_DESTINATIONS[surface];
    if (destination) return destination;
  }
  return null;
}

export function extensionDestination(
  card: Pick<ExtensionCard, "id" | "has_detail"> & { slots?: string[] },
  sections: ExtensionSettingsSection[],
): ExtensionDestination {
  const destination = destinationForSurfaces(installedSurfaces(card, sections));
  if (destination) return destination;

  // Rows that anchor nowhere are the extension's own settings, so it gets a
  // page rather than being folded into a host section.
  const section = sections.find((candidate) => candidate.id === card.id);
  const hasOwnRows = (section?.rows ?? []).some(
    (row) => !row.anchor || !ANCHOR_SURFACES.has(row.anchor),
  );
  if (card.has_detail || hasOwnRows) {
    return { kind: "extension-settings", extensionId: card.id };
  }
  return { kind: "preview" };
}

/**
 * Store entries to recommend beside a Personalize tool or Agent.
 *
 * An entry qualifies when a surface it declares resolves to this tool — never
 * because its text happens to contain a matching word. Already-installed ids
 * drop out: a recommendation to install what you have installed is noise.
 */
export function matchToolRecommendations(
  entries: StoreEntry[],
  tool: ToolSection,
  installedIds: ReadonlySet<string> = new Set(),
  limit = 3,
): StoreEntry[] {
  return entries
    .filter((entry) => {
      if (installedIds.has(entry.id)) return false;
      const destination = destinationForSurfaces(entry.extends);
      return tool === "agent"
        ? destination?.kind === "agent"
        : destination?.kind === "tools" && destination.section === tool;
    })
    .sort(
      (left, right) =>
        right.installs - left.installs || left.name.localeCompare(right.name),
    )
    .slice(0, limit);
}

export type StudioShelfMode =
  "recommendations" | "installed-with-more" | "installed";

/** The Studio shelf has one deliberately small state table.
 *
 * - No installed extension: recommend up to two and link to the store.
 * - One or two installed: show those cards and keep one Get more card.
 * - Three or more installed: every tile is real inventory, so the store card
 *   gets out of the way.
 */
export function studioShelfMode(installedCount: number): StudioShelfMode {
  if (installedCount <= 0) return "recommendations";
  if (installedCount <= 2) return "installed-with-more";
  return "installed";
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
