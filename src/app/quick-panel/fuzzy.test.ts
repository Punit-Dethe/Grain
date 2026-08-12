import { describe, expect, it } from "vitest";
import { scoreItem } from "./fuzzy";

const score = (query: string, title: string, keywords: string[] = []) =>
  scoreItem(query, title, keywords);

describe("scoreItem", () => {
  it("returns a neutral score for an empty query", () => {
    expect(score("", "Microphone")).toBe(0);
  });

  it("excludes items no token can reach", () => {
    expect(
      score("bluetooth", "Microphone", ["mic", "input device"]),
    ).toBeNull();
  });

  it("ranks a contiguous prefix above a scattered subsequence", () => {
    const prefix = score("mic", "Microphone");
    const scatter = score("mic", "Mute in call");
    expect(prefix).not.toBeNull();
    expect(scatter).not.toBeNull();
    expect(prefix!).toBeGreaterThan(scatter!);
  });

  it("matches an acronym across word boundaries", () => {
    expect(score("mtwr", "Mute while recording")).not.toBeNull();
  });

  it("matches tokens regardless of order (AND semantics)", () => {
    const ordered = score("mute recording", "Mute while recording");
    const reversed = score("recording mute", "Mute while recording");
    expect(ordered).not.toBeNull();
    expect(reversed).not.toBeNull();
  });

  it("finds a setting through an alias, not just the title", () => {
    expect(
      score("dark theme", "Appearance", ["dark mode", "theme"]),
    ).not.toBeNull();
  });

  it("tolerates a one-character typo but ranks it below a clean match", () => {
    const clean = score("recording", "Recording retention");
    const typo = score("recroding", "Recording retention");
    expect(clean).not.toBeNull();
    expect(typo).not.toBeNull();
    expect(clean!).toBeGreaterThan(typo!);
  });

  it("keeps short tokens exact (no typo tolerance under 4 chars)", () => {
    expect(score("mci", "Microphone")).toBeNull();
  });
});
