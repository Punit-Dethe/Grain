/**
 * [GRAIN] Cold-start splash teardown.
 *
 * The main settings window is DESTROYED on close to free WebView2 RAM, so every
 * reopen cold-spawns the webview and then boots Vite + React + i18n + IPC. To
 * avoid a black frame during that boot, index.html paints a pure HTML/CSS
 * loader (`#grain-splash`) that the webview shows BEFORE this JS bundle is even
 * parsed. This module tears that loader down once the live UI has painted.
 *
 * It lives in its own module (not main.tsx) because main.tsx imports App, and
 * App needs to call this — importing it back from main would be a circular dep.
 */

/**
 * Remove the static cold-start splash painted by index.html.
 *
 * @param instant - When true, remove immediately with no fade. Used by the
 *   transparent Agent windows, which must never show the opaque splash
 *   background even for a frame.
 */
export function dismissSplash(instant = false): void {
  // If the index.html script exposed the smart dismissal logic, use it:
  if (typeof (window as any).__requestSplashDismissal === "function") {
    (window as any).__requestSplashDismissal(instant);
    return;
  }

  // Fallback for when the smart script failed to load or is missing:
  const splash = document.getElementById("grain-splash");
  if (!splash) return;

  const cleanup = () => {
    splash.remove();
    // Invoke the explicit GC/memory cleanup exposed by the inline splash script
    if (typeof (window as any).__cleanupSplash === "function") {
      (window as any).__cleanupSplash();
    }
  };

  if (instant) {
    cleanup();
    return;
  }
  splash.classList.add("grain-splash-hide");
  // Drop the node after the CSS fade so it never intercepts pointer events.
  splash.addEventListener("transitionend", cleanup, { once: true });
  // Safety net in case transitionend doesn't fire (e.g. reduced-motion).
  window.setTimeout(cleanup, 400);
}
