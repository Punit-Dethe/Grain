/** Reorder visible providers while preserving the slots of unconfigured templates. */
export function reorderVisibleProviders(
  all: string[],
  visible: string[],
  moving: string,
  position: number,
): string[] {
  if (!visible.includes(moving) || !Number.isFinite(position)) return all;
  const reordered = visible.filter((id) => id !== moving);
  reordered.splice(
    Math.max(0, Math.min(Math.trunc(position), reordered.length)),
    0,
    moving,
  );
  const visibleSet = new Set(visible);
  let index = 0;
  return all.map((id) => (visibleSet.has(id) ? reordered[index++] : id));
}
