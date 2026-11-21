import { describe, expect, it } from "vitest";
import { plainFromMarkdown, resultLayout } from "./ResultCard";

describe("plainFromMarkdown", () => {
  it("drops markup but keeps lines and list dashes", () => {
    expect(
      plainFromMarkdown(
        "## Thursday\nIt is **really** `fine`, see [docs](https://x.dev).\n* one\n- two",
      ),
    ).toBe(
      "Thursday\nIt is really fine, see docs (https://x.dev).\n- one\n- two",
    );
  });

  it("keeps code inside a fence and a link that is its own label once", () => {
    expect(plainFromMarkdown("```js\nconst a = **b;\n```")).toBe(
      "const a = **b;",
    );
    expect(plainFromMarkdown("[https://x.dev](https://x.dev)")).toBe(
      "https://x.dev",
    );
    expect(plainFromMarkdown("#1 priority")).toBe("#1 priority");
  });
});

describe("resultLayout", () => {
  const base = { kind: "rewrite", subject: null, format: "plain" };
  it("picks short, document or code", () => {
    expect(resultLayout({ ...base, body: "A better sentence." })).toBe("short");
    expect(resultLayout({ ...base, body: "Line\n".repeat(6) })).toBe(
      "document",
    );
    expect(
      resultLayout({ ...base, kind: "email", subject: "Hi", body: "Short." }),
    ).toBe("document");
    expect(resultLayout({ ...base, kind: "code", body: "x" })).toBe("code");
  });
});
