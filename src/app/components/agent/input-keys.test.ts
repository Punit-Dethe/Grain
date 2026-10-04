import { describe, expect, it } from "vitest";
import { agentInputKeyAction } from "./input-keys";

describe("Agent input keyboard contract", () => {
  it("sends with Enter but leaves Shift+Enter and other keys to the field", () => {
    expect(agentInputKeyAction("Enter", false, false, 13)).toBe("send");
    expect(agentInputKeyAction("Enter", true, false, 13)).toBe("none");
    expect(agentInputKeyAction("Tab", false, false, 9)).toBe("switch");
    expect(agentInputKeyAction("Tab", true, false, 9)).toBe("none");
    expect(agentInputKeyAction("Escape", false, false, 27)).toBe("cancel");
  });
  it("does not submit or cancel while an IME is confirming or dismissing composition", () => {
    for (const key of ["Enter", "Escape", "Tab"]) {
      expect(agentInputKeyAction(key, false, true, 13)).toBe("none");
      expect(agentInputKeyAction(key, false, false, 229)).toBe("none");
    }
  });
});
