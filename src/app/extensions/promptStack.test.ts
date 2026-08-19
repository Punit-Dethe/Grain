import { describe, expect, it } from "vitest";

import type { ExtensionCard, PromptLayerInfo } from "@/bindings";
import { PROMPT_CONTEXT_SLOT, promptStackRows } from "./promptStack";

const layer = (overrides: Partial<PromptLayerInfo> = {}): PromptLayerInfo => ({
  id: "jira",
  text: "Write in imperative mood.",
  everywhere: false,
  app: [],
  website: ["jira."],
  category: [],
  ...overrides,
});

const card = (overrides: Partial<ExtensionCard> = {}): ExtensionCard => ({
  id: "com.acme.jira",
  name: "Jira Helper",
  description: "",
  version: "1.0.0",
  tier: "pack",
  trust: "community",
  overrides_installed: false,
  overridden_version: null,
  enabled: true,
  toggle_seq: "1",
  repository: null,
  capabilities: [],
  has_detail: true,
  slots: [],
  prompt_layers: [],
  ...overrides,
});

const base = {
  contextAwarenessEnabled: true,
  customProfileCount: 0,
  basePromptName: "General",
  cards: [] as ExtensionCard[],
};

describe("promptStackRows", () => {
  it("puts the user above any extension", () => {
    // The invariant, as the user reads it. If this order ever inverts, the UI
    // is telling them the opposite of what compose_prompt tells the model.
    const rows = promptStackRows({
      ...base,
      cards: [card({ prompt_layers: [layer()] })],
    });
    const sources = rows.map((row) => row.source);
    const firstExtension = sources.indexOf("extension");
    const lastYou = sources.lastIndexOf("you");
    expect(lastYou).toBeLessThan(firstExtension);
  });

  it("shows a contributing extension's text verbatim", () => {
    const rows = promptStackRows({
      ...base,
      cards: [card({ prompt_layers: [layer()] })],
    });
    const row = rows.find((r) => r.key === "ext:com.acme.jira");
    expect(row?.title).toBe("Jira Helper");
    expect(row?.layers[0]?.text).toBe("Write in imperative mood.");
  });

  it("ignores extensions that are installed but off", () => {
    const rows = promptStackRows({
      ...base,
      cards: [card({ enabled: false, prompt_layers: [layer()] })],
    });
    expect(rows.some((row) => row.source === "extension")).toBe(false);
  });

  it("names the extension that took over Grain's read of the app", () => {
    const rows = promptStackRows({
      ...base,
      cards: [card({ slots: [PROMPT_CONTEXT_SLOT] })],
    });
    const context = rows.find((row) => row.key === "context");
    expect(context?.source).toBe("extension");
    expect(context?.detail).toContain("Jira Helper");
  });

  it("marks Grain's row inactive when context awareness is off", () => {
    const rows = promptStackRows({ ...base, contextAwarenessEnabled: false });
    const context = rows.find((row) => row.key === "context");
    expect(context?.active).toBe(false);
    // Still listed: a ladder with a rung silently missing explains nothing.
    expect(context).toBeDefined();
  });

  it("counts the rules the user wrote", () => {
    const rows = promptStackRows({ ...base, customProfileCount: 1 });
    expect(rows.find((row) => row.key === "yours")?.detail).toContain("1 rule");
    expect(
      promptStackRows({ ...base, customProfileCount: 3 }).find(
        (row) => row.key === "yours",
      )?.detail,
    ).toContain("3 rules");
  });
});
