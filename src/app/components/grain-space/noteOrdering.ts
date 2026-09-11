import type { NoteCard } from "@/bindings";

export type DropEdge = "before" | "after";

/** Stable folder order: explicit positions first, then newest-first. */
export function orderFolderNotes(cards: readonly NoteCard[]): NoteCard[] {
  return [...cards].sort((a, b) => {
    const aManual = a.manual_order;
    const bManual = b.manual_order;
    if (aManual != null && bManual != null) return aManual - bManual;
    if (aManual != null) return -1;
    if (bManual != null) return 1;
    return b.timestamp - a.timestamp;
  });
}

/** Move one id around another without mutating the rendered sibling list. */
export function moveNoteId(
  ids: readonly string[],
  sourceId: string,
  targetId: string,
  edge: DropEdge,
): string[] {
  if (sourceId === targetId) return [...ids];
  const sourceIndex = ids.indexOf(sourceId);
  const targetIndex = ids.indexOf(targetId);
  if (sourceIndex < 0 || targetIndex < 0) return [...ids];

  const next = [...ids];
  next.splice(sourceIndex, 1);
  const adjustedTarget = next.indexOf(targetId);
  next.splice(adjustedTarget + (edge === "after" ? 1 : 0), 0, sourceId);
  return next;
}
