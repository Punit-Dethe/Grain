import React from "react";
import { createRoot } from "react-dom/client";
import "./i18n";
import { GrainSpaceOverlay } from "../src/components/grain-space/GrainSpaceOverlay";

/**
 * [GRAIN] The note window, as an extension surface.
 *
 * Notably NOT wrapped in `GrainSpaceHost`. That component exists to drive the
 * sleep/wake handshake over Tauri events — flush, unmount the tree, ack — and
 * inside a surface the wrapper page already does exactly that by removing the
 * iframe. Keeping it would mean two things racing to unmount the same tree.
 *
 * Everything below this line is the same code the app renders; the only thing
 * that differs is `hostAdapter`, which routes to the bridge when there is one.
 */
const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <React.StrictMode>
      <GrainSpaceOverlay />
    </React.StrictMode>,
  );
}
