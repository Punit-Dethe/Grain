import { describe, expect, it } from "vitest";

const reviewError = (body: string): string =>
  JSON.stringify({
    ...JSON.parse(body),
    approvalDigest: "a".repeat(64),
    reviewedName: "Test tools",
  });
import type {
  ExtensionCard,
  ExtensionSettingRow,
  ExtensionSettingsSection,
  StoreEntry,
} from "@/bindings";
import {
  extensionDestination,
  filterExtensions,
  nextMediaIndex,
  parseApprovalRequest,
  storeInstallAction,
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
  kind: "native",
  name: "Example Pack",
  version: "1.0.0",
  tier: "scripted",
  trust: "verified",
  previous_versions: [],
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
  icon: null,
  media: [],
  categories: [],
  extends: [],
  revocation: null,
  flags: [],
  ...overrides,
});

describe("store update actions", () => {
  it("uses signed predecessors for both extension kinds, not version text", () => {
    for (const kind of ["native", "mcp"]) {
      const listing = entry({
        kind,
        version: "1.0.0",
        previous_versions: ["2.0.0"],
      });
      expect(storeInstallAction(listing)).toBe("Install");
      expect(storeInstallAction(listing, "1.0.0")).toBe("Installed");
      expect(storeInstallAction(listing, "2.0.0")).toBe("Update");
      expect(storeInstallAction(listing, "0.9.0")).toBe("No update available");
      expect(
        storeInstallAction({ ...listing, previous_versions: [] }, "2.0.0"),
      ).toBe("No update available");
    }
  });
});

describe("extension destination routing", () => {
  it.each([
    ["snippets.after", { kind: "tools", section: "snippets" }],
    ["context.after", { kind: "tools", section: "context" }],
    ["agent.after", { kind: "agent" }],
    [
      "dictation.pipeline.after",
      { kind: "settings", section: "post-processing" },
    ],
    ["models.after", { kind: "settings", section: "capture" }],
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

  it("parses permission holds", () => {
    expect(
      parseApprovalRequest(
        reviewError('{"needsPermissions":["storage","open:url"]}'),
      ),
    ).toEqual({
      approvalDigest: "a".repeat(64),
      reviewedName: "Test tools",
      permissions: ["storage", "open:url"],
      promptLayers: [],
      actions: [],
      authentication: null,
      recommendation: false,
    });
  });

  it("parses an approval that is only about prompt text", () => {
    // An inert pack asks for no capability at all, so a parser keyed on
    // permissions alone would read this as "nothing to approve" and the enable
    // would fail with a raw JSON string.
    expect(
      parseApprovalRequest(
        reviewError(
          '{"needsPermissions":[],"needsPromptLayers":[{"id":"jira","target":"additive","text":"Be terse.","everywhere":false,"app":[],"website":["jira."],"category":[]}]}',
        ),
      ),
    ).toEqual({
      approvalDigest: "a".repeat(64),
      reviewedName: "Test tools",
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
      reviewError(
        '{"needsPermissions":[],"needsActions":[{"id":"next","title":"Skip to the next track","confirms":false,"everywhere":true,"app":[],"website":[]}]}',
      ),
    );
    expect(parsed?.actions).toHaveLength(1);
    expect(parsed?.actions[0]?.title).toBe("Skip to the next track");
  });

  it("opens approval for recommendation or authentication alone", () => {
    expect(
      parseApprovalRequest(reviewError('{"needsRecommendation":true}')),
    ).toEqual({
      approvalDigest: "a".repeat(64),
      reviewedName: "Test tools",
      permissions: [],
      promptLayers: [],
      actions: [],
      authentication: null,
      recommendation: true,
    });
    expect(
      parseApprovalRequest(
        reviewError(
          '{"needsAuthentication":{"provider_name":"GitHub","scopes":["read:user"],"api_hosts":["api.github.com"],"authorization_host":"github.com","token_host":"github.com"}}',
        ),
      ),
    ).toEqual({
      approvalDigest: "a".repeat(64),
      reviewedName: "Test tools",
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
        reviewError(
          '{"needsPermissions":["storage"],"needsAuthentication":{"provider_name":"GitHub","scopes":[42]}}',
        ),
      ),
    ).toBeNull();
  });

  it("refuses permission sheets without a valid package review identity", () => {
    for (const approvalDigest of [
      undefined,
      null,
      12,
      "",
      "a".repeat(63),
      "g".repeat(64),
    ]) {
      expect(
        parseApprovalRequest(
          JSON.stringify({ needsPermissions: ["storage"], approvalDigest }),
        ),
      ).toBeNull();
    }
    expect(
      parseApprovalRequest(reviewError('{"needsPermissions":["storage"]}'))
        ?.approvalDigest,
    ).toBe("a".repeat(64));
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
