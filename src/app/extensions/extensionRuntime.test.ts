import { describe, expect, it } from "vitest";
import type {
  ExtensionCard,
  ExtensionSettingRow,
  ExtensionSettingsSection,
  StoreEntry,
} from "@/bindings";
import {
  extensionDestination,
  filterExtensions,
  matchToolRecommendations,
  nextMediaIndex,
  parseApprovalRequest,
  promptLayerScope,
  parseSlotConflict,
  studioShelfMode,
} from "./extensionRuntime";

const card = (overrides: Partial<ExtensionCard> = {}): ExtensionCard => ({
  id: "example.pack",
  name: "Example Pack",
  description: "An example extension",
  icon: null,
  version: "1.0.0",
  tier: "scripted",
  trust: "community",
  overrides_installed: false,
  overridden_version: null,
  enabled: true,
  toggle_seq: "1",
  repository: null,
  capabilities: [],
  has_detail: false,
  slots: [],
  prompt_layers: [],
  actions: [],
  kind: "standalone",
  recommend: null,
  needs: [],
  auto_send_eligible: false,
  auto_send_enabled: false,
  auto_send_note: null,
  ...overrides,
});

const row = (anchor: string | null): ExtensionSettingRow => ({
  key: "enabled",
  label: "Enabled",
  description: "",
  kind: "bool",
  anchor,
  order: 0,
  value: true,
  notice: null,
  min: null,
  max: null,
  step: null,
  options: [],
  fields: [],
  item_label: null,
});

const sections = (anchor: string | null): ExtensionSettingsSection[] => [
  { id: "example.pack", name: "Example Pack", rows: [row(anchor)] },
];

const entry = (overrides: Partial<StoreEntry> = {}): StoreEntry => ({
  id: "example.pack",
  name: "Example Pack",
  version: "1.0.0",
  tier: "scripted",
  trust: "verified",
  capabilities: [],
  description: "An example extension",
  repo: "",
  size: "1 MB",
  author: "Grain Labs",
  reviewed_at: "",
  reviewed_commit: "",
  stars: 0,
  installs: 0,
  readme: "",
  media: [],
  categories: [],
  extends: [],
  revocation: null,
  flags: [],
  ...overrides,
});

describe("extension destination routing", () => {
  it.each([
    ["snippets.after", { kind: "tools", section: "snippets" }],
    ["context.after", { kind: "tools", section: "context" }],
    ["agent.after", { kind: "tools", section: "agent" }],
    [
      "dictation.pipeline.after",
      { kind: "settings", section: "post-processing" },
    ],
    ["models.after", { kind: "settings", section: "speech-to-text" }],
    ["grainspace.after", { kind: "notes-settings" }],
  ])("routes %s contributions", (anchor, expected) => {
    expect(extensionDestination(card(), sections(anchor))).toEqual(expected);
  });

  it("uses standalone settings for unanchored rows", () => {
    expect(extensionDestination(card(), sections(null))).toEqual({
      kind: "extension-settings",
      extensionId: "example.pack",
    });
  });

  it("opens a preview when no configuration destination exists", () => {
    expect(extensionDestination(card(), [])).toEqual({ kind: "preview" });
  });
});

describe("tool recommendations", () => {
  it.each([
    [0, "recommendations"],
    [1, "installed-with-more"],
    [2, "installed-with-more"],
    [3, "installed"],
    [8, "installed"],
  ] as const)(
    "uses the correct Studio shelf for %i installed",
    (count, mode) => {
      expect(studioShelfMode(count)).toBe(mode);
    },
  );

  it("recommends by the surface an extension declares, not by its wording", () => {
    // App Modes anchors itself to `context.after` and never uses the word
    // "context"; Starter Prompts feeds the prompt list and says "prompts" in
    // every sentence. Keyword scoring got both of these backwards.
    const appModes = entry({
      id: "grain.app-modes",
      name: "App Modes",
      description: "Format what you dictate differently in each app.",
      extends: ["context.after"],
    });
    const starterPrompts = entry({
      id: "grain.starter-prompts",
      name: "Starter Prompts",
      description: "General, Coding and Email prompts for post-processing.",
      extends: ["dictation.prompts"],
    });
    expect(
      matchToolRecommendations([starterPrompts, appModes], "context"),
    ).toEqual([appModes]);
    expect(
      matchToolRecommendations([starterPrompts, appModes], "agent"),
    ).toEqual([]);
  });

  it("does not recommend what is already installed", () => {
    const voiceActions = entry({
      id: "grain.voice-actions",
      extends: ["snippets.after"],
    });
    expect(matchToolRecommendations([voiceActions], "snippets")).toEqual([
      voiceActions,
    ]);
    expect(
      matchToolRecommendations(
        [voiceActions],
        "snippets",
        new Set(["grain.voice-actions"]),
      ),
    ).toEqual([]);
  });

  it("limits Studio to the two most-installed matching extensions", () => {
    const entries = [
      entry({ id: "context.one", installs: 10, extends: ["context.after"] }),
      entry({ id: "context.two", installs: 80, extends: ["context.after"] }),
      entry({ id: "context.three", installs: 40, extends: ["context.after"] }),
    ];

    expect(matchToolRecommendations(entries, "context", new Set(), 2)).toEqual([
      entries[1],
      entries[2],
    ]);
  });

  it("declares nothing for a tool no surface maps to", () => {
    expect(
      matchToolRecommendations(
        [entry({ extends: ["snippets.after"] })],
        "dictionary",
      ),
    ).toEqual([]);
  });
});

describe("extension collection helpers", () => {
  it("searches names, ids, and descriptions directly", () => {
    const entries = [
      entry({ id: "voice.actions", name: "Voice Actions" }),
      entry({ id: "app.modes", name: "App Modes" }),
    ];
    expect(filterExtensions(entries, "voice")).toEqual([entries[0]]);
    expect(filterExtensions(entries, "app.modes")).toEqual([entries[1]]);
  });

  it("wraps media carousel movement", () => {
    expect(nextMediaIndex(0, 3, -1)).toBe(2);
    expect(nextMediaIndex(2, 3, 1)).toBe(0);
    expect(nextMediaIndex(4, 0, 1)).toBe(0);
  });

  it("parses permission and slot holds", () => {
    expect(
      parseApprovalRequest('{"needsPermissions":["storage","open:url"]}'),
    ).toEqual({
      permissions: ["storage", "open:url"],
      promptLayers: [],
      actions: [],
      authentication: null,
      recommendation: false,
    });
    expect(
      parseSlotConflict(
        '{"slotConflict":{"slot":"output.destination","currentOccupant":"old"}}',
      ),
    ).toEqual({ slot: "output.destination", currentOccupant: "old" });
  });

  it("parses an approval that is only about prompt text", () => {
    // An inert pack asks for no capability at all, so a parser keyed on
    // permissions alone would read this as "nothing to approve" and the enable
    // would fail with a raw JSON string.
    expect(
      parseApprovalRequest(
        '{"needsPermissions":[],"needsPromptLayers":[{"id":"jira","target":"additive","text":"Be terse.","everywhere":false,"app":[],"website":["jira."],"category":[]}]}',
      ),
    ).toEqual({
      permissions: [],
      promptLayers: [
        {
          id: "jira",
          target: "additive",
          text: "Be terse.",
          everywhere: false,
          app: [],
          website: ["jira."],
          category: [],
        },
      ],
      actions: [],
      authentication: null,
      recommendation: false,
    });
    expect(parseApprovalRequest('{"needsPermissions":[]}')).toBeNull();
    expect(parseApprovalRequest("not json")).toBeNull();
  });

  it("parses an approval that is only about what an extension can do", () => {
    // The third thing one enable can be refused for. Keyed on actions alone
    // because an extension can declare one while asking for no capability the
    // sheet would otherwise mention.
    const parsed = parseApprovalRequest(
      '{"needsPermissions":[],"needsActions":[{"id":"next","title":"Skip to the next track","confirms":false,"everywhere":true,"app":[],"website":[]}]}',
    );
    expect(parsed?.actions).toHaveLength(1);
    expect(parsed?.actions[0]?.title).toBe("Skip to the next track");
  });

  it("opens approval for recommendation or authentication alone", () => {
    expect(parseApprovalRequest('{"needsRecommendation":true}')).toEqual({
      permissions: [],
      promptLayers: [],
      actions: [],
      authentication: null,
      recommendation: true,
    });
    expect(
      parseApprovalRequest(
        '{"needsAuthentication":{"provider_name":"GitHub","scopes":["read:user"],"api_hosts":["api.github.com"],"authorization_host":"github.com","token_host":"github.com"}}',
      ),
    ).toEqual({
      permissions: [],
      promptLayers: [],
      actions: [],
      authentication: {
        provider_name: "GitHub",
        scopes: ["read:user"],
        api_hosts: ["api.github.com"],
        authorization_host: "github.com",
        token_host: "github.com",
      },
      recommendation: false,
    });
    expect(
      parseApprovalRequest(
        '{"needsPermissions":["storage"],"needsAuthentication":{"provider_name":"GitHub","scopes":[42]}}',
      ),
    ).toBeNull();
  });

  it("describes when a contributed layer applies", () => {
    const layer = {
      id: "a",
      target: "additive",
      text: "Be terse.",
      everywhere: false,
      app: ["code"],
      website: [],
      category: [],
    };
    expect(promptLayerScope(layer)).toBe("Adds a rule · In code");
    expect(promptLayerScope({ ...layer, everywhere: true })).toBe(
      "Adds a rule · Every dictation",
    );
    expect(promptLayerScope({ ...layer, target: "main" })).toBe(
      "Replaces Main prompt · In code",
    );
    expect(promptLayerScope({ ...layer, target: "context" })).toBe(
      "Replaces Context provider · In code",
    );
  });
});

describe("extension settings routing", () => {
  it("still prefers an extension's own page when it has one", () => {
    expect(
      extensionDestination(
        card({ id: "with.page", has_detail: true, slots: [] }),
        [],
      ),
    ).toEqual({ kind: "extension-settings", extensionId: "with.page" });
  });
});
