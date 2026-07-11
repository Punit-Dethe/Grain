import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { GrainSpaceOverlay } from "./GrainSpaceOverlay";
import { flushBridge } from "./sleepBridge";

/** Backend lifecycle events (see src-tauri/src/grain_space/window.rs). */
const SLEEP_EVENT = "grain-space://sleep";
const REVIVE_EVENT = "grain-space://revive";

/**
 * [GRAIN] The workspace window's mount host. The window itself survives its
 * "close" hidden (hide-don't-destroy, for instant re-summon), so idle RAM is
 * reclaimed here instead: on the backend's sleep event the pending edits are
 * flushed and the ENTIRE React tree is unmounted — the DOM collapses to an
 * empty root and the JS heap becomes garbage-collectable, right before the
 * backend suspends the webview. On revive the tree remounts (milliseconds)
 * and the ack tells the backend the window can be shown already painted.
 */
export function GrainSpaceHost() {
  const [awake, setAwake] = useState(true);

  useEffect(() => {
    const unlistens = [
      listen(SLEEP_EVENT, () => {
        void (async () => {
          // Never let a failed flush block the sleep — the backend hides the
          // window after a fallback timeout regardless.
          await flushBridge.flush?.().catch(() => undefined);
          setAwake(false);
        })();
      }),
      listen(REVIVE_EVENT, () => setAwake(true)),
    ];
    return () => {
      unlistens.forEach((p) => void p.then((fn) => fn()));
    };
  }, []);

  // Acks run AFTER the commit: ui_ready once the UI exists (backend reveals
  // the window), sleep_ready once the tree is gone (backend hides + suspends).
  useEffect(() => {
    if (awake) {
      void commands.grainSpaceUiReady();
    } else {
      void commands.grainSpaceSleepReady();
    }
  }, [awake]);

  return awake ? <GrainSpaceOverlay /> : null;
}
