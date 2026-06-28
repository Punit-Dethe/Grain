import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { AgentPalette } from "./components/agent/AgentPalette";
import { AgentPanel } from "./components/agent/AgentPanel";
import { initUiScale } from "./lib/utils/uiScale";
import { dismissSplash } from "./lib/utils/splash";

// Set platform before render so CSS can scope per-platform (e.g. scrollbar styles)
document.documentElement.dataset.platform = platform();

// Initialize i18n (both the main settings window and the Agent use it)
import "./i18n";

import { useModelStore } from "./stores/modelStore";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

// [GRAIN] One Vite entry serves every window; we branch on the window label. The
// Agent's two surfaces (the centred `agent` palette and the right-side
// `agent-panel`) are frameless, transparent, summoned on demand and DESTROYED on
// close, so they skip the main app's heavy init (UI scaling, model store) and drop
// the beige page background the main window paints behind its webview.
const winLabel = getCurrentWindow().label;
if (winLabel === "agent" || winLabel === "agent-panel") {
  document.documentElement.dataset.window = winLabel;
  document.documentElement.style.background = "transparent";
  document.body.style.background = "transparent";
  // Agent windows are transparent + frameless; the splash background would show
  // through as an opaque block, so drop it instantly with no fade.
  dismissSplash(true);
  root.render(
    <React.StrictMode>
      {winLabel === "agent" ? <AgentPalette /> : <AgentPanel />}
    </React.StrictMode>,
  );
} else {
  // [GRAIN] The main window is opaque + OS-rounded (DWM corner preference in
  // lib.rs). Unlike the Agent branch it is NOT transparent, so the page keeps
  // its background to fill client area outside the rounded React card.
  //
  // Scale the whole UI to the screen so it looks the same apparent size on
  // any display (and never gigantic on a small/low-res one). Run before render
  // so the first paint is already scaled.
  initUiScale();

  // Initialize model store (loads models and sets up event listeners)
  useModelStore.getState().initialize();

  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );

  // The splash is dismissed by App itself once onboarding state resolves (so it
  // never lifts to reveal the blank `null` frame App renders while that IPC
  // round-trip is in flight). See the dismissSplash call in App.tsx.
}
