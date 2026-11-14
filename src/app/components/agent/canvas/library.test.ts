import { describe, expect, it } from "vitest";
import { createParser } from "@openuidev/react-lang";
import { CANVAS_EXAMPLES, canvasLibrary, canvasSpec } from "./library";
import { canvasProgram, withoutSpeech } from "./program";

const parse = (text: string) =>
  createParser(canvasLibrary.toJSONSchema(), canvasLibrary.root).parse(text);

describe("canvas library", () => {
  it("builds a component reference the host accepts", () => {
    const spec = canvasSpec();
    // MAX_SPEC_BYTES in grain_agent_canvas.rs.
    expect(new TextEncoder().encode(spec).length).toBeLessThan(96 * 1024);
    for (const name of ["Canvas", "ListItem", "Button", "Form", "Stat"])
      expect(spec).toContain(name);
  });

  it("parses the prompt's own examples without errors", () => {
    for (const example of CANVAS_EXAMPLES) {
      expect(canvasSpec()).toContain(example);
      const result = parse(example);
      expect(result.meta.errors).toEqual([]);
      expect(result.root?.typeName).toBe("Canvas");
    }
  });

  it("finds the program in a fenced or chatty reply", () => {
    const program = 'root = Canvas([Heading("Hi")])';
    expect(canvasProgram(program)).toBe(program);
    expect(canvasProgram("```openui\n" + program + "\n```")).toBe(program);
    expect(canvasProgram("Here you go:\n\n" + program)).toBe(program);
    expect(canvasProgram("Just a sentence.")).toBeNull();
    expect(canvasProgram("SAY: Sunny today.\n" + program)).toBe(program);
  });

  it("never shows the voice line", () => {
    expect(withoutSpeech("SAY: Sunny today.\nroot = Canvas([])")).toBe(
      "root = Canvas([])",
    );
    expect(withoutSpeech("Plain answer.")).toBe("Plain answer.");
  });

  it("parses the designed views and building blocks", () => {
    const result = parse(
      [
        "root = Canvas([rate, pic, card, row])",
        'rate = Hero("USD to INR", "₹83.42", "Up 0.3% today", "Updated now", "up", "dollar")',
        'pic = Image("https://upload.wikimedia.org/x.jpg", "Beach", "Goa", "wide")',
        'card = Card([Icon("sun", "l"), Stat("High", "31°", "warmer", "up"), Badge("Open", "good")])',
        'row = List([ListItem("Baga", "Beach", "4 km", "map-pin", @ToAssistant("More on Baga"), "https://upload.wikimedia.org/y.jpg")])',
      ].join("\n"),
    );
    expect(result.meta.errors).toEqual([]);
    expect(result.root?.typeName).toBe("Canvas");
  });

  it("keeps decoration out of the model's hands", () => {
    const spec = canvasSpec();
    for (const view of ["Weather", "Places", "Journey", "Hero"])
      expect(spec).toContain(view);
    expect(spec).not.toMatch(/\b(teal|indigo|orange|pink)\b/);
  });
});
