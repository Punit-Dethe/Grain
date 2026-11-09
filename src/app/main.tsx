import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";
import { initUiScale } from "./lib/utils/uiScale";

// Set platform before render so CSS can scope per-platform (e.g. scrollbar styles)
document.documentElement.dataset.platform = platform();

// Initialize i18n (both the main settings window and the Agent use it)
import "./i18n";

import { useModelStore } from "./stores/modelStore";

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement,
);

// [GRAIN] The main window is opaque + OS-rounded (DWM corner preference in
// lib.rs). Unlike the Agent branch it is NOT transparent, so the page keeps
// its background to fill client area outside the rounded React card.
//
// Pin the rem baseline to a FIXED 16px — deliberately NOT driven by the
// screen. (This comment used to claim the opposite, that the UI scales to the
// display; it has said so since before the scaled canvas was removed, and
// uiScale.ts explains at length why viewport-driven rem is exactly what that
// removal was undoing. Left uncorrected it would have talked the UI 2.0
// rewrite into putting the behaviour back.) Run before render so the first
// paint is already correct.
initUiScale();

// Initialize model store (loads models and sets up event listeners)
useModelStore.getState().initialize();

// [GRAIN] UI 2.0 is the only tree. The pre-2.0 tree and its `VITE_GRAIN_UI`
// switch are gone — a fallback nobody can reach is just a second UI to keep
// compiling. Still a dynamic import so the shell stays out of the entry chunk.
void import("./GrainApp").then(({ default: MainApp }) => {
  root.render(
    <React.StrictMode>
      <MainApp />
    </React.StrictMode>,
  );
});
