// [GRAIN] Quick Panel → settings deep-link focus.
//
// A setting is reached by navigating to its section (a hash change) and then
// scrolling to its row. The two steps are decoupled because the section may not
// be mounted yet when the Quick Panel closes: the request is parked here and
// consumed by SettingsPage once the matching section has rendered.

import type { SettingsSectionId } from "../navigation";

interface FocusRequest {
  section: SettingsSectionId;
  /** The already-translated setting title — matched against rendered headings. */
  title: string;
}

let pending: FocusRequest | null = null;

/** Park a focus request; SettingsPage picks it up after the section mounts. */
export function requestSettingFocus(
  section: SettingsSectionId,
  title: string,
): void {
  pending = { section, title };
}

/** Take the pending request iff it targets `section` (clears it). */
export function consumeSettingFocus(
  section: SettingsSectionId,
): FocusRequest | null {
  if (pending && pending.section === section) {
    const request = pending;
    pending = null;
    return request;
  }
  return null;
}

/**
 * Scroll to the setting whose heading text equals `title` within the active
 * settings pane and pulse it. Best-effort: returns false if the row is not on
 * screen yet (the caller retries while async panes settle).
 */
export function focusSettingByTitle(title: string): boolean {
  const scope = document.querySelector<HTMLElement>(".settings-content");
  if (!scope) return false;

  const wanted = title.trim();
  const nodes = scope.querySelectorAll<HTMLElement>("h1, h2, h3, label");
  let target: HTMLElement | null = null;
  for (const node of nodes) {
    if ((node.textContent ?? "").trim() === wanted) {
      target = node;
      break;
    }
  }
  if (!target) {
    for (const node of nodes) {
      if ((node.textContent ?? "").trim().startsWith(wanted)) {
        target = node;
        break;
      }
    }
  }
  if (!target) return false;

  const row =
    target.closest<HTMLElement>(".setting-row") ??
    target.closest<HTMLElement>(".surface-well") ??
    target;
  row.scrollIntoView({ behavior: "smooth", block: "center" });
  row.classList.add("qp-focus-pulse");
  window.setTimeout(() => row.classList.remove("qp-focus-pulse"), 1600);
  return true;
}
