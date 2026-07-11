/**
 * [GRAIN] Host ↔ overlay bridge for the sleep cycle: the overlay registers its
 * save-flush here so the window host can persist pending edits BEFORE it
 * unmounts the whole tree (the DOM purge) when the backend asks it to sleep.
 * A module-level cell instead of context — the host must reach it while the
 * overlay is being torn down.
 */
export const flushBridge: { flush: (() => Promise<void>) | null } = {
  flush: null,
};
