import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { viteSingleFile } from "vite-plugin-singlefile";
import path from "node:path";

/**
 * [GRAIN] Builds the note UI into ONE self-contained HTML document, which is
 * what a pack's `uiSource` is (NOTE-UI-EXTENSION-PLAN.md).
 *
 * The output goes straight into the pack project, so `grain-ext build-pack`
 * inlines the same file the app's own components produced — there is no second
 * copy of the UI to keep in step.
 */
export default defineConfig({
  root: __dirname,
  plugins: [react(), viteSingleFile()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "../src") },
  },
  build: {
    outDir: path.resolve(__dirname, "../grain-extensions/core/grain.note-ui"),
    emptyOutDir: false,
    // One file: no chunking, no asset URLs. The surface has an opaque origin
    // and cannot fetch anything relative to itself.
    assetsInlineLimit: 100_000_000,
    cssCodeSplit: false,
    rollupOptions: {
      // `ui.html` because that is what the manifest names; vite would emit
      // index.html and the pack would then reference a file that is not there.
      input: path.resolve(__dirname, "index.html"),
      output: { inlineDynamicImports: true },
    },
  },
});
