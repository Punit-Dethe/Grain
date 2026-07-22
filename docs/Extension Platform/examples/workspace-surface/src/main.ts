grain.onShortcut(async (id) => {
  if (id !== "open") return;
  await grain.workspace.open({
    message: `Opened by ${grain.extId}`,
  });
});
