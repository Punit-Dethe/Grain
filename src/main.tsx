import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { AgentPanel } from "./components/agent/AgentPanel";
import { initUiScale } from "./lib/utils/uiScale";

// Set platform before render so CSS can scope per-platform (e.g. scrollbar styles)
document.documentElement.dataset.platform = platform();

// Initialize i18n (both the main settings window and the Agent use it)
import "./i18n";

import { useModelStore } from "./stores/modelStore";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

// [GRAIN] One Vite entry serves every window; we branch on the window label. The
// Agent's reply surface (`agent-panel`) is frameless, transparent, summoned on
// demand and DESTROYED on close, so it skips the main app's heavy init (UI
// scaling, model store) and drops the beige page background the main window
// paints behind its webview. (The summon INPUT is native — it lives in the pill
// process, not a webview.)
const winLabel = getCurrentWindow().label;
if (winLabel === "agent-panel") {
  document.documentElement.dataset.window = winLabel;
  document.documentElement.style.background = "transparent";
  document.body.style.background = "transparent";
  root.render(
    <React.StrictMode>
      <AgentPanel />
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
}
