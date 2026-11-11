import { useEffect, useRef } from "react";
import { type UnlistenFn } from "@tauri-apps/api/event";
import { events } from "@/bindings";
import { audioAmplitude } from "../../overlay/wave";

/** Rate (per second) the light rises toward a louder voice and settles back. */
const RISE = 18;
const SETTLE = 4;

/**
 * The notch's bottom fade, lifted and brightened by the microphone level.
 * Writes only transform/opacity (compositor work) and stops its frame loop
 * once it has settled after `active` turns off.
 */
export function AgentVoiceFade({ active }: { active: boolean }) {
  const light = useRef<HTMLSpanElement>(null);
  const target = useRef(0);
  // What is on screen, kept across `active` changes so it settles, not snaps.
  const shown = useRef(0);

  useEffect(() => {
    if (!active) {
      target.current = 0;
      return;
    }
    let disposed = false;
    let release: UnlistenFn | undefined;
    void events.micLevel
      .listen(({ payload }) => {
        // Speech sits low on the RMS scale; the root lifts it into view.
        target.current = Math.min(1, Math.sqrt(audioAmplitude(payload)) * 1.25);
      })
      .then((fn) => {
        if (disposed) fn();
        else release = fn;
      })
      .catch(console.error);
    return () => {
      disposed = true;
      release?.();
      target.current = 0;
    };
  }, [active]);

  useEffect(() => {
    const element = light.current;
    if (!element || matchMedia("(prefers-reduced-motion: reduce)").matches)
      return;
    let level = shown.current;
    let frame = 0;
    let previous = performance.now();
    const tick = (now: number) => {
      const dt = Math.min((now - previous) / 1000, 0.1);
      previous = now;
      const goal = target.current;
      const rate = goal > level ? RISE : SETTLE;
      level += (goal - level) * (1 - Math.exp(-rate * dt));
      shown.current = level;
      element.style.transform = `scaleY(${0.3 + level * 0.7})`;
      element.style.opacity = String(level);
      // Idle and silent: stop until `active` restarts the loop.
      if (!active && level < 0.005) {
        element.style.opacity = "0";
        return;
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [active]);

  return <span ref={light} className="agent-notch-voice" aria-hidden="true" />;
}
