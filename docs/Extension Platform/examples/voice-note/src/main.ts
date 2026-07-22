grain.onSessionStage(async (text, { mode, signal }) => {
  if (mode !== "note" || signal.aborted) return text;

  const note = await grain.llm.complete(
    [
      "Turn this dictation into a concise Markdown note.",
      "Keep facts and intent unchanged. Add a short title as the first line.",
      "",
      text,
    ].join("\n"),
  );
  if (signal.aborted) return text;

  const capturedAt = new Date().toISOString();
  await grain.doc.put(`voice-note-${Date.now()}`, {
    capturedAt,
    original: text,
    note,
  });
  if (signal.aborted) return text;

  await grain.log.info(`Stored voice note captured at ${capturedAt}`);
  return { handled: true };
});
