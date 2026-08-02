import { defineConfig } from "vitest/config";
import path from "node:path";

/**
 * [GRAIN] Unit tests for frontend logic.
 *
 * Scoped to `src/**` on purpose: `tests/` belongs to Playwright, and a runner
 * that picks up the other runner's specs fails in a way that looks like a
 * broken test rather than a misconfigured glob.
 */
export default defineConfig({
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src/app") },
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    environment: "node",
    // [GRAIN] The suite is currently empty: its only subject was the note UI's
    // in-app/in-surface adapter, which went away when the workspace became a tab
    // and stopped needing to run in two hosts. Keeping the runner green rather
    // than failing on "no test files" — the folder-tree and drag-target logic
    // landing next is what it is for.
    passWithNoTests: true,
  },
});
