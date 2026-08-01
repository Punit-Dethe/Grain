import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AgentPanel } from "./components/agent/AgentPanel";
import { initUiScale } from "./lib/utils/uiScale";

// Set platform before render so CSS can scope per-platform (e.g. scrollbar styles)
document.documentElement.dataset.platform = platform();

// Initialize i18n (both the main settings window and the Agent use it)
import "./i18n";

import { useModelStore } from "./stores/modelStore";

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement,
);

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
  // [GRAIN] There is no `grain-space` branch any more. The notes workspace was a
  // second frameless window with a sleep/revive handshake to reclaim its idle
  // RAM; it is now the Notes tab of this window, where leaving the tab unmounts
  // the tree and reclaims the same memory without a second webview.
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

  // [GRAIN] UI 2.0 is now the default tree. The old tree is kept as an opt-in
  // fallback (`VITE_GRAIN_UI=legacy`) rather than deleted yet; flattening
  // src/next → src and removing the old tree is a later, separate cleanup. The
  // build-time constant still lets Vite split and tree-shake the unused tree.
  const mainTree =
    import.meta.env.VITE_GRAIN_UI === "legacy"
      ? import("./App")
      : import("./next/NextApp");

  void mainTree.then(({ default: MainApp }) => {
    root.render(
      <React.StrictMode>
        <MainApp />
      </React.StrictMode>,
    );
  });
}
