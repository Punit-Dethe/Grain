import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  // Path aliases
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
      "@/bindings": resolve(__dirname, "./src/bindings.ts"),
    },
  },

  // Entry points: the main settings window, plus the hidden extension-host
  // supervisor page (SPEC §3.1) — a real route Tauri loads as its own webview.
  // The recording overlay is retired — grain-pill (native) is the only overlay.
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        extensionHost: resolve(__dirname, "extension-host.html"),
        // The wrapper around an extension's workspace UI (SPEC §7.1) — its own
        // page so extension markup never shares Grain's main global.
        extensionSurface: resolve(__dirname, "extension-surface.html"),
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
