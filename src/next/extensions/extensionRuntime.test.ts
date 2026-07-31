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
  parseNeedsPermissions,
  parseSlotConflict,
  recommendationScore,
} from "./extensionRuntime";

const card = (overrides: Partial<ExtensionCard> = {}): ExtensionCard => ({
  id: "example.pack",
  name: "Example Pack",
  description: "An example extension",
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
  ui_source: null,
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
  installs: 0,
  readme: "",
  media: [],
  categories: [],
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
  it("ranks exact categories above conservative copy matches", () => {
    const exact = entry({
      id: "exact",
      name: "Vocabulary Kit",
      categories: ["dictionary"],
    });
    const copy = entry({
      id: "copy",
      name: "Spelling Helper",
      description: "Improves dictionary spelling",
    });
    expect(recommendationScore(exact, "dictionary")).toBeGreaterThan(
      recommendationScore(copy, "dictionary"),
    );
    expect(matchToolRecommendations([copy, exact], "dictionary")).toEqual([
      exact,
      copy,
    ]);
  });

  it("does not recommend unrelated catalogue entries", () => {
    expect(
      matchToolRecommendations(
        [entry({ name: "Recording Theme", description: "A new pill color" })],
        "snippets",
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
      parseNeedsPermissions('{"needsPermissions":["storage","open:url"]}'),
    ).toEqual(["storage", "open:url"]);
    expect(
      parseSlotConflict(
        '{"slotConflict":{"slot":"pill.theme","currentOccupant":"old"}}',
      ),
    ).toEqual({ slot: "pill.theme", currentOccupant: "old" });
  });
});
