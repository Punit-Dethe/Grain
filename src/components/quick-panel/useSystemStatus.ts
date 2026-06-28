/**
 * [GRAIN] Quick Panel system-status feed for the bottom status bar.
 *
 * Design philosophy (see AGENTS.md §2):
 *  - DECOUPLED: derives everything from signals the webview ALREADY receives.
 *    It adds NO new backend events, NO WebSocket client, and NO new long-lived
 *    listeners beyond the two webview events the app already emits:
 *      • `model-state-changed`  (model load / unload lifecycle — already the
 *        source of truth for modelStore)
 *      • `history-update-payload` (a transcription / processing just landed —
 *        already consumed by useHistory)
 *    Route / provider facts come straight from the in-memory pool stores.
 *  - DESTROY IF NOT IN USE: every listener is torn down on unmount and the
 *    transient "pulse" timer is always cleared.
 *  - LOW NOISE: one prioritised status at a time, with a calm idle default.
 *
 * Priority (highest wins): error > model loading > transient activity pulse
 * (transcribed / processed) > model loaded/unloaded route state > idle.
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { events, type HistoryUpdatePayload } from "@/bindings";
import { useSttPoolStore } from "@/stores/sttPoolStore";
import { usePpPoolStore } from "@/stores/ppPoolStore";

/** Visual tone for the indicator dot — maps to an accent colour + animation. */
export type StatusTone = "idle" | "busy" | "active" | "ok" | "error";

export interface SystemStatus {
  /** Short ALL-CAPS label shown in the status bar. */
  label: string;
  tone: StatusTone;
}

interface ModelStateEvent {
  event_type: string;
  model_id?: string | null;
  model_name?: string | null;
  error?: string | null;
}

/** How long a transient "transcribed / processed" pulse stays on screen. */
const PULSE_MS = 2600;

/** Tracks the model lifecycle from the existing `model-state-changed` event. */
type ModelPhase = "idle" | "loading" | "loaded" | "error";

export const useSystemStatus = (): SystemStatus => {
  const [modelPhase, setModelPhase] = useState<ModelPhase>("idle");
  const [modelError, setModelError] = useState<string | null>(null);
  /** A short-lived activity message (e.g. "TRANSCRIBED") or null when calm. */
  const [pulse, setPulse] = useState<string | null>(null);
  const pulseTimer = useRef<number | null>(null);

  // Route facts (no extra fetch — these stores are already live in the panel).
  const sttRotation = useSttPoolStore((s) => s.smartRotation);
  const cloudProviders = useSttPoolStore((s) => s.cloudProviders);
  const ppRotation = usePpPoolStore((s) => s.smartRotation);

  // --- Model lifecycle: reuse the SAME webview event modelStore listens to. ---
  useEffect(() => {
    const unlisten = listen<ModelStateEvent>("model-state-changed", (e) => {
      switch (e.payload.event_type) {
        case "loading_started":
          setModelError(null);
          setModelPhase("loading");
          break;
        case "loading_completed":
          setModelError(null);
          setModelPhase("loaded");
          break;
        case "unloaded":
          setModelPhase("idle");
          break;
        case "loading_failed":
          setModelError(e.payload.error ?? "Model error");
          setModelPhase("error");
          break;
        default:
          break;
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // --- Activity pulse: reuse the SAME history event useHistory listens to. ---
  useEffect(() => {
    const flash = (msg: string) => {
      setPulse(msg);
      if (pulseTimer.current) window.clearTimeout(pulseTimer.current);
      pulseTimer.current = window.setTimeout(() => setPulse(null), PULSE_MS);
    };
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        const e = payload.entry;
        flash(
          (e.post_processed_text?.trim().length ?? 0) > 0
            ? "PROCESSED"
            : "TRANSCRIBED",
        );
      } else if (payload.action === "updated") {
        if ((payload.entry.post_processed_text?.trim().length ?? 0) > 0) {
          flash("PROCESSED");
        }
      }
    });
    return () => {
      unlisten.then((fn) => fn());
      if (pulseTimer.current) window.clearTimeout(pulseTimer.current);
    };
  }, []);

  // --- Resolve a single prioritised status. ---
  if (modelPhase === "error") {
    return { label: "MODEL ERROR", tone: "error" };
  }
  if (modelPhase === "loading") {
    return { label: "LOADING MODEL", tone: "busy" };
  }
  if (pulse) {
    return { label: pulse, tone: "ok" };
  }

  // Calm steady-state: describe the active route.
  const enabledCloud = cloudProviders.filter((p) => p.enabled ?? true).length;
  if (sttRotation) {
    const ppNote = ppRotation ? " + LLM" : "";
    if (enabledCloud === 0) {
      return { label: "ROTATION: NO PROVIDER ON", tone: "busy" };
    }
    return {
      label: `ROTATING ${enabledCloud} CLOUD${ppNote}`,
      tone: "active",
    };
  }
  if (modelPhase === "loaded") {
    return { label: "MODEL LOADED // IDLE", tone: "ok" };
  }
  return { label: "LOCAL // STANDBY", tone: "idle" };
};
