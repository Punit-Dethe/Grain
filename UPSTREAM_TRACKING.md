# Upstream Tracking (Handy)

This document tracks updates and commits from the upstream `Handy` repository since June 5, 2026. This ensures Grain stays reasonably up-to-date with upstream improvements while maintaining its own architectural independence.

## 1. Updates Completed / Merged
Updates from upstream that have been successfully ported, refactored, and merged into Grain.

- **`debug + perf transcribe cli (#1541)`** (Jun 25)
  - Merged in Grain as: `feat: port upstream live log viewer and perf transcribe cli`
- **`fix: stop overlay mic-level events leaking memory when overlay disabled (#1447)`** (Jun 24)
  - Merged in Grain as: `fix: upstream merges - stop mic-level Tauri memory leak (#1447)...`
- **`fix: skip post-processing when transcription is empty (#1537)`** (Jun 24)
  - Merged in Grain as: `fix: upstream merges - ... and skip empty post-processing (#1537)`
- **`live debug log viewer in app (#1535)`** (Jun 23)
  - Merged in Grain as: `feat: port upstream live log viewer and perf transcribe cli`

---

## 2. Updates Not Completed (Pending)
Updates that exist in Handy's history since June 5, but have not yet been evaluated, merged, or ported to Grain.

- **`Clarified branding and redistribution terms for Handy`** (Jun 23)
  - *Note: Likely irrelevant to Grain, but kept here for tracking until formally ignored.*
- **`fix: dropdown overflow in post-processing settings (#1402)`** (Jun 18)
- **`fix(models): show size for downloaded models (#1484)`** (Jun 11)
- **`fix(visualizer): scale FFT window to device sample rate (#1491)`** (Jun 11)

---

## 3. Updates Intentionally Ignored
Updates from upstream that we have evaluated and explicitly decided NOT to merge (e.g., conflicts with Grain's architecture, UI philosophy, or native implementations).

- *(None yet)*
