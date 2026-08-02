import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  PUBLIC_EXTENSION_TOKENS,
  serializeExtensionPalette,
} from "@/lib/extensionTheme";

const stylesheet = readFileSync(new URL("./app.css", import.meta.url), "utf8");

function themeValues(theme: "light" | "dark"): Record<string, string> {
  const selector = `.grain-root[data-theme="${theme}"]`;
  const start = stylesheet.indexOf(selector);
  expect(start).toBeGreaterThanOrEqual(0);
  const blockStart = stylesheet.indexOf("{", start);
  const blockEnd = stylesheet.indexOf("}", blockStart);
  const block = stylesheet.slice(blockStart + 1, blockEnd);

  return Object.fromEntries(
    [...block.matchAll(/(--color-[\w-]+)\s*:\s*([^;]+);/g)].map(
      ([, name, value]) => [name, value.trim()],
    ),
  );
}

describe("UI 2.0 extension token contract", () => {
  it.each(["light", "dark"] as const)(
    "emits all eight non-empty public tokens in %s mode",
    (theme) => {
      const values = themeValues(theme);
      const palette = serializeExtensionPalette((name) => values[name] ?? "");
      const declarations = new Map(
        palette.split(";").map((declaration) => {
          const separator = declaration.indexOf(":");
          return [
            declaration.slice(0, separator),
            declaration.slice(separator + 1),
          ];
        }),
      );

      expect(declarations.size).toBe(8);
      for (const token of PUBLIC_EXTENSION_TOKENS) {
        expect(declarations.get(`--grain-${token}`)?.trim()).toBeTruthy();
      }
    },
  );

  it("does not emit empty values that would suppress extension fallbacks", () => {
    expect(serializeExtensionPalette(() => "   ")).toBe("");
  });
});
