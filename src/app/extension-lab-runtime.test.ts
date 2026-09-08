import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";

type Match = { id: string; score: number; margin?: number };
type Decision = { pick: string } | { ambiguous: string[] } | { none: true };
type Reply = { message?: string; error?: string };

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

function runtime(options: {
  extensionId?: string;
  lexical?: Match[] | Error;
  semantic?: Match[] | Error;
}) {
  let onRequest: ((request: string) => Promise<Reply>) | undefined;
  const semantic = options.semantic ?? [];
  const lexical = options.lexical ?? [];
  const grain = {
    match: {
      lexical:
        lexical instanceof Error
          ? vi.fn().mockRejectedValue(lexical)
          : vi.fn().mockResolvedValue(lexical),
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
    onRequest(handler: typeof onRequest) {
      onRequest = handler;
    },
  };
  const extensionSource = source.replace(
    "__GRAIN_LAB_EXTENSION_ID__",
    options.extensionId ?? "com.grain.lab.stream-music",
  );
  new Function("grain", extensionSource)(grain);
  if (!onRequest) throw new Error("lab request handler was not registered");
  return { onRequest, grain };
}

describe("Recommendation Lab internal command diagnostic", () => {
  it.each([
    ["com.grain.lab.stream-music", 9],
    ["com.grain.lab.music-library", 11],
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

  it.each([
    ["play", "play-track", "Play song"],
    ["pause song", "pause-playback", "Pause song"],
    ["resume song", "resume-playback", "Resume song"],
    ["next song", "next-track", "Next song"],
    ["previous song", "previous-track", "Previous song"],
  ])(
    "exposes the explicit playback control '%s'",
    async (utterance, id, title) => {
      const lab = runtime({
        lexical: [{ id, score: 1 }],
        semantic: new Error("model unavailable"),
      });

      const reply = await lab.onRequest(utterance);
      const candidates = lab.grain.match.lexical.mock.calls[0][1] as {
        id: string;
        phrases: string[];
      }[];

      expect(candidates).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id,
            phrases: expect.arrayContaining([utterance]),
          }),
        ]),
      );
      expect(reply.message).toContain(`1. ${title}`);
    },
  );

  it("keeps the five core playback controls inside Music Library too", async () => {
    const lab = runtime({
      extensionId: "com.grain.lab.music-library",
      semantic: new Error("model unavailable"),
    });

    await lab.onRequest("play song");
    const candidates = lab.grain.match.lexical.mock.calls[0][1] as {
      id: string;
      phrases: string[];
    }[];

    for (const [id, phrase] of [
      ["play-local-track", "play song"],
      ["pause-playback", "pause song"],
      ["resume-playback", "resume song"],
      ["next-track", "next song"],
      ["previous-track", "previous song"],
    ]) {
      expect(candidates).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id,
            phrases: expect.arrayContaining([phrase]),
          }),
        ]),
      );
    }
  });

  it("supplies realistic full utterances for the streaming playback controls", async () => {
    const lab = runtime({ semantic: new Error("model unavailable") });

    await lab.onRequest("can you play the next song from Spotify?");
    const candidates = lab.grain.match.semantic.mock.calls[0][1] as {
      id: string;
      examples: string[];
    }[];

    for (const id of [
      "play-track",
      "pause-playback",
      "resume-playback",
      "next-track",
      "previous-track",
    ]) {
      expect(
        candidates.find((candidate) => candidate.id === id)!.examples.length,
      ).toBeGreaterThanOrEqual(6);
    }
    expect(
      candidates.find((candidate) => candidate.id === "next-track")!.examples,
    ).toEqual(
      expect.arrayContaining([
        "could you move on to the following song",
        "go to the next song on Spotify",
      ]),
    );
  });

  it("marks a safe, semantically clear command as Executed and Auto-send", async () => {
    const lab = runtime({
      lexical: [{ id: "search-catalog", score: 0.9 }],
      semantic: [
        { id: "search-catalog", score: 0.82 },
        { id: "play-track", score: 0.6 },
      ],
    });

    const reply = await lab.onRequest("find this album without playing it");

    expect(reply.message).toContain("command decision: executed");
    expect(reply.message).toContain("Auto-send");
    expect(lab.grain.match.decide).toHaveBeenCalledWith(expect.any(Array), {
      minConfidence: 0.5,
      margin: 0.15,
    });
  });

  it("reports close commands as ambiguous without exposing extension UI", async () => {
    const lab = runtime({
      semantic: [
        { id: "play-track", score: 0.72 },
        { id: "queue-track", score: 0.69 },
        { id: "play-playlist", score: 0.51 },
      ],
    });
    const suggested = await lab.onRequest("play this one, maybe put it next");

    expect(suggested.message).toContain("command decision: suggested");
    expect(suggested).not.toHaveProperty("view");
  });

  it("never lets lexical evidence reorder a clear semantic command", async () => {
    const lab = runtime({
      lexical: [{ id: "play-track", score: 1 }],
      semantic: [
        { id: "next-track", score: 0.82 },
        { id: "play-track", score: 0.75 },
      ],
    });

    const reply = await lab.onRequest(
      "can you play the next song from Spotify?",
    );

    expect(reply.message).toContain("command decision: executed");
    expect(reply.message).toContain("1. Next song");
    expect(lab.grain.match.decide).toHaveBeenNthCalledWith(
      1,
      [
        { id: "next-track", score: 0.82 },
        { id: "play-track", score: 0.75 },
      ],
      { minConfidence: 0.5, margin: 0.05 },
    );
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

    expect(reply.message).toContain("command decision: executed");
    expect(reply.message).toContain("semantic 79.0%, lexical —");
  });

  it("never executes from lexical evidence when semantic matching is unavailable", async () => {
    const lab = runtime({
      lexical: [{ id: "next-track", score: 1 }],
      semantic: new Error("model unavailable"),
    });

    const reply = await lab.onRequest("skip track");

    expect(reply.message).toContain("Semantic matching is unavailable");
    expect(reply.message).toContain("semantic —, lexical 100.0%");
    expect(reply).not.toHaveProperty("view");
  });

  it("makes a host rejection of both matching APIs explicit", async () => {
    const lab = runtime({
      lexical: new Error("unknown method"),
      semantic: new Error("unknown method"),
    });

    const reply = await lab.onRequest("play song");

    expect(reply.message).toContain(
      "Both matching signals are unavailable. No command ran.",
    );
  });
});
