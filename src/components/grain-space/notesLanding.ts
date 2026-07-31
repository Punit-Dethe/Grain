import type { Note } from "@/bindings";

export type NotesLanding =
  | { kind: "note"; id: string }
  | { kind: "draft" }
  | { kind: "none" };

/**
 * Choose the editor's first document without coupling navigation policy to the
 * workspace lifecycle. UI 2.0 honors a backend focus handoff directly and
 * otherwise opens ready to write. Legacy keeps its original constraint: a
 * focus id must appear in a successful card listing, with the first card as its
 * stale-focus fallback.
 */
export function resolveNotesLanding(
  variant: "default" | "next",
  focusNoteId: string | null,
  firstCardId?: string,
  listingAvailable = true,
  focusIsListed = false,
): NotesLanding {
  if (variant === "next") {
    return focusNoteId ? { kind: "note", id: focusNoteId } : { kind: "draft" };
  }
  if (!listingAvailable) return { kind: "none" };
  if (focusNoteId && focusIsListed) {
    return { kind: "note", id: focusNoteId };
  }
  if (firstCardId) return { kind: "note", id: firstCardId };
  return { kind: "draft" };
}

/** A local-only draft. Its empty id keeps it off disk until the first edit. */
export function createBlankDraft(now = Date.now()): Note {
  return {
    id: "",
    title: "",
    tldr: "",
    body: "",
    timestamp: now,
    todo_tags: [],
    reminder_state: { status: "none", fire_at: null },
    is_pinned: false,
  };
}
