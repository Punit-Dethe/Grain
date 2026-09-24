import { expect, it } from "vitest";
import { formatKeyCombination, getKeyName } from "./keyboard";

const compoundKeys = [
  ["ScrollLock", "scrolllock", "Scroll Lock"],
  ["CapsLock", "capslock", "Caps Lock"],
  ["NumLock", "numlock", "Num Lock"],
  ["PageUp", "pageup", "Page Up"],
  ["PageDown", "pagedown", "Page Down"],
  ["PrintScreen", "printscreen", "Print Screen"],
] as const;

for (const [code, stored, displayed] of compoundKeys) {
  it(`stores ${code} in a parseable form`, () => {
    expect(getKeyName({ code } as KeyboardEvent)).toBe(stored);
    expect(formatKeyCombination(stored, "linux")).toBe(displayed);
  });
}
