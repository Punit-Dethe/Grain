/**
 * [GRAIN] Root rem baseline.
 *
 * Scaling is now owned by `ScaledStage` (a single CSS transform on a fixed
 * 1280×760 design canvas, used by BOTH the Quick Panel and the Settings view).
 * The window is locked to the 1280:760 aspect ratio, so the stage scales the
 * whole UI up/down as one unit with no letterboxing — the look is preserved at
 * any window size.
 *
 * Because the transform handles apparent sizing, the root font-size is just a
 * FIXED rem baseline the rem-based Settings components are authored against
 * (16px). It is no longer driven by the screen size — doing both would compound.
 * The Quick Panel uses absolute px and is unaffected by this value.
 */

const BASE_PX = 16;

export function applyUiScale(): void {
  document.documentElement.style.fontSize = `${BASE_PX}px`;
}

export function initUiScale(): void {
  applyUiScale();
}
