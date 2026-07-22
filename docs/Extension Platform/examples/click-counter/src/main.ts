grain.onShortcut(async (id) => {
  if (id !== "count") return;

  const previous = (await grain.storage.get<number>("presses")) ?? 0;
  const presses = previous + 1;
  await grain.storage.set("presses", presses);
  await grain.log.info(`Shortcut pressed ${presses} time(s)`);
});
