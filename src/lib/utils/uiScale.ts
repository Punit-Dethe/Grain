/**
 * [GRAIN] Root rem baseline.
 *
 * A FIXED 16px baseline, deliberately not driven by the screen size.
 *
 * This used to be one of two sizing mechanisms: the settings view was a fixed
 * 1280×760 canvas under a single `transform: scale()` (`ScaledStage`), with the
 * main window locked to that aspect ratio so the letterboxing never showed, and
 * this file kept the rem baseline constant so the two would not compound. The
 * scaled canvas is gone from the main window — Grain Note lives there now, and
 * people maximise it to write — so the app reflows like an ordinary window and
 * this is the only sizing mechanism left.
 *
 * Keep it fixed. Driving rem off the viewport would reintroduce exactly the
 * problem the un-scaling removed: a bigger window would grow the type instead of
 * showing more content. The Quick Panel is unaffected either way — it is
 * absolute px on its own scaled canvas.
 */

const BASE_PX = 16;

export function applyUiScale(): void {
  document.documentElement.style.fontSize = `${BASE_PX}px`;
}

export function initUiScale(): void {
  applyUiScale();
}
