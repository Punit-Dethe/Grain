import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";

type Match = { id: string; score: number; margin?: number };
type Decision = { pick: string } | { ambiguous: string[] } | { none: true };
type ViewNode = {
  type: string;
  text?: string;
  label?: string;
  value?: string;
  children?: ViewNode[];
};
type View = {
  root: ViewNode;
  actions: { id: string; label: string }[];
};
type Reply = { view?: View; message?: string; error?: string };
type ViewEvent = {
  kind: "submit" | "cancel" | "change";
  target?: string;
  values?: Record<string, string | boolean>;
};

const source = readFileSync(
  new URL("../../src-tauri/src/extension_lab_runtime.js", import.meta.url),
  "utf8",
);

function decide(
  candidates: Match[],
  policy: { minConfidence?: number; margin?: number } = {},
): Decision {
  const floor = policy.minConfidence ?? 0;
  const margin = policy.margin ?? 0;
  const eligible = [...candidates]
    .filter((candidate) => candidate.score >= floor)
    .sort(
      (left, right) =>
        right.score - left.score || left.id.localeCompare(right.id),
    );
  if (!eligible.length) return { none: true };
  if (
    eligible.length === 1 ||
    eligible[0].score - eligible[1].score >= margin
  ) {
    return { pick: eligible[0].id };
  }
  return {
    ambiguous: eligible
      .filter((candidate) => eligible[0].score - candidate.score < margin)
      .map((candidate) => candidate.id),
  };
}

function nodes(root: ViewNode): ViewNode[] {
  return [root, ...(root.children ?? []).flatMap(nodes)];
}

function badges(reply: Reply): string[] {
  return nodes(reply.view!.root)
    .filter((node) => node.type === "badge")
    .map((node) => node.text ?? "");
}

function metadata(reply: Reply): string[] {
  return nodes(reply.view!.root)
    .filter((node) => node.type === "metadata")
    .map((node) => `${node.label}: ${node.value}`);
}

function runtime(options: {
  extensionId?: string;
  lexical?: Match[];
  semantic?: Match[] | Error;
}) {
  let onRequest: ((request: string) => Promise<Reply>) | undefined;
  let onEvent:
    | ((event: ViewEvent) => Reply | void | Promise<Reply | void>)
    | undefined;
  const semantic = options.semantic ?? [];
  const grain = {
    match: {
      lexical: vi.fn().mockResolvedValue(options.lexical ?? []),
      semantic:
        semantic instanceof Error
          ? vi.fn().mockRejectedValue(semantic)
          : vi.fn().mockResolvedValue(semantic),
      decide: vi.fn(
        async (
          candidates: Match[],
          policy?: { minConfidence?: number; margin?: number },
        ) => decide(candidates, policy),
      ),
    },
    ui: {
      onEvent(handler: typeof onEvent) {
        onEvent = handler;
      },
    },
    onRequest(handler: typeof onRequest) {
      onRequest = handler;
    },
  };
  const extensionSource = source.replace(
    "__GRAIN_LAB_EXTENSION_ID__",
    options.extensionId ?? "com.grain.lab.stream-music",
  );
  new Function("grain", extensionSource)(grain);
  if (!onRequest || !onEvent)
    throw new Error("lab handlers were not registered");
  return { onRequest, onEvent, grain };
}

describe("Recommendation Lab internal command diagnostic", () => {
  it.each([
    ["com.grain.lab.stream-music", 7],
    ["com.grain.lab.music-library", 7],
    ["com.grain.lab.issue-tracker", 8],
    ["com.grain.lab.code-host", 8],
    ["com.grain.lab.translator", 6],
  ])(
    "registers the challenging %s command corpus",
    async (extensionId, count) => {
      const lab = runtime({
        extensionId,
        semantic: new Error("model unavailable"),
      });

      await lab.onRequest("diagnostic request");

      expect(lab.grain.match.semantic).toHaveBeenCalledWith(
        "diagnostic request",
        expect.arrayContaining([
          expect.objectContaining({
            id: expect.any(String),
            examples: expect.any(Array),
          }),
        ]),
      );
      expect(lab.grain.match.semantic.mock.calls[0][1]).toHaveLength(count);
    },
  );

  it("marks a safe, semantically clear command as Executed and Auto-send", async () => {
    const lab = runtime({
      lexical: [{ id: "search-catalog", score: 0.9 }],
      semantic: [
        { id: "search-catalog", score: 0.82 },
        { id: "play-track", score: 0.6 },
      ],
    });

    const reply = await lab.onRequest("find this album without playing it");

    expect(badges(reply)).toEqual(["Executed", "Auto-send"]);
    expect(lab.grain.match.decide).toHaveBeenCalledWith(expect.any(Array), {
      minConfidence: 0.5,
      margin: 0.15,
    });
  });

  it("surfaces close commands as Suggested and resolves the choice in-place", async () => {
    const lab = runtime({
      semantic: [
        { id: "play-track", score: 0.72 },
        { id: "queue-track", score: 0.69 },
        { id: "play-playlist", score: 0.51 },
      ],
    });
    const suggested = await lab.onRequest("play this one, maybe put it next");

    expect(badges(suggested)).toEqual(["Suggested"]);
    expect(suggested.view!.actions.map((action) => action.id)).toEqual([
      "choose-play-track",
      "choose-queue-track",
      "cancel-diagnostic",
    ]);

    const resolved = await lab.onEvent({
      kind: "submit",
      target: "choose-queue-track",
      values: {},
    });
    expect(badges(resolved as Reply)).toEqual(["Executed", "User selected"]);
  });

  it("rejects a command choice that the validated view did not offer", async () => {
    const lab = runtime({
      semantic: [
        { id: "play-track", score: 0.72 },
        { id: "queue-track", score: 0.69 },
      ],
    });
    await lab.onRequest("play this one next");

    const reply = await lab.onEvent({
      kind: "submit",
      target: "choose-merge-pull-request",
      values: {},
    });

    expect(reply).toEqual({
      error: "The selected command was not offered by this view.",
    });
    expect(
      await lab.onEvent({
        kind: "submit",
        target: "choose-play-track",
        values: {},
      }),
    ).toBeUndefined();
  });

  it("executes a semantic paraphrase with no lexical hit", async () => {
    const lab = runtime({
      lexical: [],
      semantic: [
        { id: "play-playlist", score: 0.79 },
        { id: "play-track", score: 0.52 },
      ],
    });

    const reply = await lab.onRequest(
      "put on the collection I made for dinner",
    );

    expect(badges(reply)).toEqual(["Executed"]);
    expect(metadata(reply)[0]).toContain("semantic 79.0% · lexical —");
  });

  it("never executes from lexical evidence when semantic matching is unavailable", async () => {
    const lab = runtime({
      lexical: [{ id: "skip-track", score: 1 }],
      semantic: new Error("model unavailable"),
    });

    const reply = await lab.onRequest("skip track");

    expect(badges(reply)).toEqual(["Semantic unavailable"]);
    expect(reply.view!.actions[0].id).toBe("finish-diagnostic");
    expect(metadata(reply)[0]).toContain("semantic — · lexical 100.0%");
  });
});
