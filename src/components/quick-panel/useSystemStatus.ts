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
 * Steady state: a persistent two-segment route line describing BOTH pipelines,
 * read like the panel's signal chain —
 *   `TRANSCRIPTION: <route> // PROCESSING: <route>`
 * Transient events (model loading, transcribed, processed, error) surface over
 * that line for a moment and then fall back to it.
 *
 * Priority (highest wins): error > model loading > transient activity pulse
 * (transcribed / processed) > persistent route line.
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { events, type HistoryUpdatePayload } from "@/bindings";
import { useSttPoolStore } from "@/stores/sttPoolStore";
import { usePpPoolStore } from "@/stores/ppPoolStore";
import { useModelStore } from "@/stores/modelStore";

/** Visual tone for the indicator dot — maps to an accent colour + animation. */
export type StatusTone = "idle" | "busy" | "active" | "ok" | "error";

export interface SystemStatus {
  /** Short ALL-CAPS label shown in the status bar. */
  label: string;
  tone: StatusTone;
  /** True when this is a momentary event surfacing over the route line. */
  transient: boolean;
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
  const ppProviders = usePpPoolStore((s) => s.providers);
  const ppProvidersWithKeys = usePpPoolStore((s) => s.providersWithKeys);
  const ppSelectedId = usePpPoolStore((s) => s.selectedProviderId);
  const models = useModelStore((s) => s.models);
  const currentModel = useModelStore((s) => s.currentModel);

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

  // --- Transient events surface over the route line, then fall back. ---
  if (modelPhase === "error") {
    return { label: "MODEL ERROR", tone: "error", transient: true };
  }
  if (modelPhase === "loading") {
    return { label: "LOADING MODEL", tone: "busy", transient: true };
  }
  if (pulse) {
    return { label: pulse, tone: "ok", transient: true };
  }

  // --- Persistent steady-state: describe BOTH routes in one line. ---

  // Transcription route: rotation ON → rotating across enabled cloud providers;
  // OFF → the local in-process model.
  let stt: string;
  if (sttRotation) {
    const enabledCloud = cloudProviders.filter((p) => p.enabled ?? true).length;
    stt =
      enabledCloud === 0
        ? "ROTATE (NO PROVIDER ON)"
        : `ROTATE ${enabledCloud} CLOUD`;
  } else {
    const name = models.find((m) => m.id === currentModel)?.name;
    stt = name ? `LOCAL · ${name.toUpperCase()}` : "LOCAL";
  }

  // Processing route: rotation ON → rotating across enabled CONFIGURED
  // providers; OFF → the selected provider (must have a key). No key anywhere
  // → "NO PROVIDER".
  const configured = ppProviders.filter((p) => ppProvidersWithKeys.has(p.id));
  let pp: string;
  if (configured.length === 0) {
    pp = "NO PROVIDER";
  } else if (ppRotation) {
    const enabledPp = configured.filter((p) => p.enabled).length;
    pp =
      enabledPp === 0 ? "ROTATE (NONE ON)" : `ROTATE ${enabledPp} PROVIDER`;
  } else {
    const sel =
      configured.find((p) => p.id === ppSelectedId) ??
      configured.find((p) => p.enabled) ??
      configured[0];
    pp = sel ? sel.label.toUpperCase() : "NO PROVIDER";
  }

  const noProvider = stt.includes("NO PROVIDER") || pp === "NO PROVIDER";
  return {
    label: `${stt} // ${pp}`,
    tone: noProvider ? "idle" : "active",
    transient: false,
  };
};
