/** IME confirmation must not become an Agent submit or cancel. */
export function agentInputKeyAction(
  key: string,
  shift: boolean,
  composing: boolean,
  keyCode: number,
): "send" | "cancel" | "switch" | "none" {
  if (composing || keyCode === 229) return "none";
  if (key === "Escape") return "cancel";
  if (key === "Enter" && !shift) return "send";
  if (key === "Tab" && !shift) return "switch";
  return "none";
}
