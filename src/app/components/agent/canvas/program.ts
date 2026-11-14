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
