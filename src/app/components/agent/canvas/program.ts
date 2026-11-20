/** The OpenUI Lang program in a model reply, or `null` when the reply is not
 * one (it then renders as plain text). Models sometimes fence the code or put
 * a sentence before it despite being told not to. */
export function canvasProgram(reply: string): string | null {
  let text = reply.trim();
  const fenced = /```[a-z-]*\s*\n([\s\S]*?)```/i.exec(text);
  if (fenced) text = fenced[1].trim();
  const start = text.search(/^root\s*=/m);
  return start < 0 ? null : text.slice(start).trim();
}

/** A reply without its leading voice line (`SAY: …`). */
export function withoutSpeech(reply: string): string {
  return reply.replace(/^\s*say:[^\n]*(\n|$)/i, "").trim();
}

const unquote = (text: string) => text.replace(/\\(.)/g, "$1").trim() || null;

export type ViewKind = "weather" | "places" | "journey" | "hero" | "view";

/** What a generated view is about, for its card in the chat: the first
 * designed view's kind and headline (its first text argument). */
export function viewSummary(program: string): {
  kind: ViewKind;
  title: string | null;
} {
  const quoted = '"((?:[^"\\\\]|\\\\.)*)"';
  const view = new RegExp(
    `\\b(Weather|Places|Journey|Hero)\\(\\s*${quoted}`,
  ).exec(program);
  if (view)
    return {
      kind: view[1].toLowerCase() as ViewKind,
      title: unquote(view[2]),
    };
  const heading = new RegExp(`\\bHead(?:er|ing)\\(\\s*${quoted}`).exec(program);
  return { kind: "view", title: heading ? unquote(heading[1]) : null };
}
