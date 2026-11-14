import { describe, expect, it } from "vitest";
import { createParser } from "@openuidev/react-lang";
import { CANVAS_EXAMPLES, canvasLibrary, canvasSpec } from "./library";
import { canvasProgram, viewSummary, withoutSpeech } from "./program";

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

  it("parses a place with an icon instead of a photo", () => {
    const result = parse(
      [
        'root = Canvas([Places("Cafés", "2 open", [a, b])])',
        'a = Place("Blue Tokai", "Coffee · quiet", "1 km", "Open", "good", null, null, "star")',
        'b = Place("Third Wave", "Coffee", "2 km")',
      ].join("\n"),
    );
    expect(result.meta.errors).toEqual([]);
  });

  it("tells the model the space a view must fit", () => {
    const spec = canvasSpec();
    expect(spec).toContain("never scrolls");
    expect(spec).toContain("Places 5 rows");
  });

  it("names a view for its card in the chat", () => {
    expect(viewSummary(CANVAS_EXAMPLES[0])).toEqual({
      kind: "weather",
      title: "Pune",
    });
    expect(viewSummary(CANVAS_EXAMPLES[1])).toEqual({
      kind: "places",
      title: "Ski areas near you",
    });
    expect(viewSummary('root = Canvas([Heading("Say \\"hi\\"")])')).toEqual({
      kind: "view",
      title: 'Say "hi"',
    });
    expect(viewSummary("root = Canvas([])")).toEqual({
      kind: "view",
      title: null,
    });
  });

  it("keeps decoration out of the model's hands", () => {
    const spec = canvasSpec();
    for (const view of ["Weather", "Places", "Journey", "Hero"])
      expect(spec).toContain(view);
    expect(spec).not.toMatch(/\b(teal|indigo|orange|pink)\b/);
  });
});
