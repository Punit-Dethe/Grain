export const MAX_CUSTOM_WORD_LENGTH = 50;

/** Normalize every Grain-owned dictionary entry point the same way. Phrases
 * are intentional: the backend's n-gram correction supports them. */
export function normalizeCustomWord(value: string): string {
  return value
    .replace(/[<>"']/g, "")
    .replace(/\s+/g, " ")
    .trim();
}
