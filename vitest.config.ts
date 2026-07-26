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
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    environment: "node",
  },
});
