export const RECENT_NOTES_PAGE_SIZE = 5;

/**
 * Keep the visible recent-note window bounded by the current corpus while
 * preserving the five-row first page.
 */
export function clampRecentNotesVisibleCount(
  visibleCount: number,
  totalCount: number,
  pageSize = RECENT_NOTES_PAGE_SIZE,
): number {
  if (totalCount <= 0) return 0;
  return Math.min(totalCount, Math.max(pageSize, visibleCount));
}

/** Reveal one more recent-note page without overshooting the corpus. */
export function revealMoreRecentNotes(
  visibleCount: number,
  totalCount: number,
  pageSize = RECENT_NOTES_PAGE_SIZE,
): number {
  const current = clampRecentNotesVisibleCount(
    visibleCount,
    totalCount,
    pageSize,
  );
  return Math.min(totalCount, current + pageSize);
}
