import { describe, expect, it } from "vitest";
import { createParser } from "@openuidev/react-lang";
import {
  CANVAS_EXAMPLES,
  canvasLibrary,
  canvasSpec,
  isFigureColumn,
  splitFigure,
  statLayout,
} from "./library";
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
        'rate = Hero("USD to INR", "₹83.42", "Up 0.3% today", "good", "Updated now", null, "up", "dollar")',
        'pic = Image("https://upload.wikimedia.org/x.jpg", "Beach", "Goa", "wide")',
        'card = Card([Header("Goa", "Beach day"), Stats([Stat("High", "31°", "warmer", "warn", "up"), Stat("UV", "9")]), Badge("Open", "good")])',
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
    expect(spec).toContain('"wide": a short landscape panel');
  });

  it("parses images in every shape", () => {
    const shelf = parse(
      `root = Canvas([films, list])
films = Stack([Image("https://a.org/p.jpg", "Dune", "Dune", "tall")], "row")
list = List([ListItem("Blinding Lights", "The Weeknd", "1", null, null, "https://a.org/c.jpg", "square"), ListItem("Dune", "2021", null, null, null, "https://a.org/d.jpg", "tall")])`,
    );
    expect(shelf.meta.errors).toEqual([]);
  });

  it("parses the wide layout and rejects an unknown one", () => {
    const wide = parse('root = Canvas([Heading("Hi")], "wide")');
    expect(wide.meta.errors).toEqual([]);
    expect(wide.root?.props.layout).toBe("wide");
    const tall = parse('root = Canvas([Heading("Hi")])');
    expect(tall.meta.errors).toEqual([]);
    expect(parse('root = Canvas([], "huge")').meta.errors).not.toEqual([]);
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

  it("fits every stat value on one line at one size", () => {
    // Short figures stay large, all in one row.
    expect(statLayout(["61", "65", "128"], 440)).toEqual({
      cols: 3,
      size: "l",
    });
    // Longer ones shrink together rather than wrap.
    expect(statLayout(["Monthly", "20K", "50K/mo", "2K/min"], 400).cols).toBe(
      4,
    );
    // Four long values go two by two before getting tiny.
    expect(
      statLayout(
        ["Resets monthly", "20K of 100K", "50K a month", "2K/min"],
        440,
      ),
    ).toEqual({ cols: 2, size: "m" });
  });

  it("caps a view at a few blocks and keeps its buttons", () => {
    const spec = canvasSpec();
    expect(spec).toContain("ONE IDEA PER VIEW");
    expect(spec).not.toMatch(/\bIcon\(/);
    expect(viewSummary(CANVAS_EXAMPLES[5])).toEqual({
      kind: "view",
      title: "Ramayana · IMAX 3D",
    });
  });

  it("sets a figure's unit apart from its number", () => {
    expect(splitFigure("41 ms")).toEqual(["41", "ms"]);
    expect(splitFigure("128 µg/m³")).toEqual(["128", "µg/m³"]);
    expect(splitFigure("1.2M/min")).toEqual(["1.2M", "/min"]);
    for (const whole of ["99.94%", "24°", "₹430", "3h 15m", "Monthly", "21.2M"])
      expect(splitFigure(whole)).toEqual([whole, ""]);
    expect(isFigureColumn(["188 ms", "0.02%", "₹430", "—"])).toBe(true);
    expect(isFigureColumn(["edge-01", "Mumbai"])).toBe(false);
  });

  it("keeps decoration out of the model's hands", () => {
    const spec = canvasSpec();
    for (const view of ["Weather", "Places", "Journey", "Hero"])
      expect(spec).toContain(view);
    expect(spec).not.toMatch(/\b(teal|indigo|orange|pink)\b/);
  });
});
