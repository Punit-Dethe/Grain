# Single-model dictation convergence

User-authorized implementation, 6 November 2026. Working branch:
`converge/handy-dictation-20261106`, based on `origin/main` at `b40330eef`.
Handy reference: `82dfab9fda18c4bc24e1f6e56d36474d38eca0bc`; adopted
backend baseline: `73c095857bc4eadff097f64767c40ad129cc372d`.

## Result required

One selected dictation model, one everyday dictation shortcut, one catalogue.
Handy's advertised model capabilities select streaming versus offline VAD and
its existing live/chunked decode policy. Grain does not maintain another
streaming action, registry view, model slot or AI start-mode selector.
Reviewed Parakeet TDT v2/v3 artifacts retain the existing Flow route; translation
continues through the shared dictation action. Resolve the action when recording
starts and retain it until stop/release. Keep Prompt Record, AI processing,
optional AI finish, Agent, Paste Catch and Grain's presentation preferences.

The user's explicit request permits adopting Handy's catalogue UI, sorting and
language filtering on this surface despite the general frontend freeze. It
does not authorize replacing the rest of Grain's UI, automatic recognition
language detection/model switching, native inference upgrades or release work.

## Execution order

1. Remove the separate Native ASR action, commands, model field, shortcut and
   AI start-mode preference. Restore Handy's selection and auto-selection for
   every supported model. No legacy settings migration or second route.
2. Use Handy's shared capture/VAD/live decode path. Retain only the small Flow
   router and necessary Grain product hooks. Make live overlay presentation
   depend on selected model capability, with compact Flow presentation.
3. Adopt Handy's model table and catalogue helpers; adapt only imports, Grain
   theme and existing store/command boundaries. Use the same catalogue package.
   Remove the duplicated Standard/Streaming libraries and onboarding split.
4. Update current tests and maintenance contracts together. Run focused core,
   routing, catalogue and frontend checks, then backend checks and the upstream
   ratchet. Review the final diff. Do not build a new harness or installer.
5. Commit and push the isolated branch. Give the user a development command and
   a short real-device checklist; do not merge to main without instruction.

## Acceptance checkpoints

- A streaming model and a batch model can each be downloaded/selected through
  the same API and visible catalogue; selection persists in `selected_model`.
- One dictation binding works with either; startup uses Handy's capability VAD
  and live decode policy, with worker cleanup on cancellation/start failure.
- Reviewed Flow artifacts route to Flow, translation bypasses Flow, unrelated
  custom models cannot claim Flow. Mid-recording model changes cannot change
  the retained stop action.
- Prompt Record and AI finish use that same selected model; live/compact/hidden
  pill preferences remain coherent. Agent and file transcription retain their
  existing admission/resource guards.
- Catalogue search, language ranking, download/cancel/select/delete, missing
  model and load failure behavior remain covered by existing tests/checks.
- User device acceptance: batch dictation; streaming dictation/live text;
  Flow; cancellation and repeated use; AI/Prompt Record; catalogue visuals.

## Status and verification

Implementation complete; device/visual acceptance is pending. The graph indexes
only a small subset of this checkout, so its zero-node review result is not
acceptance evidence. Source/dependency inspection and the following checks were
used instead:

- Real Windows backend test executable, via the maintained manifest-aware runner:
  **796 passed, 29 ignored**. Ignored hardware/native-fixture checks were not
  claimed as passed. Includes capture/coordinator, overlay, model availability,
  file-job admission, cancellation and history regressions.
- grain-core: **241 passed**, including exact Flow qualification, shared model
  routing, shortcut contract and settings persistence tests.
- Frontend unit tests: **115 passed**. Catalogue grouping/custom model/download
  checks and file-job snapshot/event/unmount/failure races are included.
- Windows production-configuration `cargo check --lib`, TypeScript, frontend
  production build, focused ESLint, catalogue sync,
  all 23 locale structural checks, settings reachability and frontend freeze pass.
  Build retains the existing bundle-size warning.
- `policy.rs`, `live.rs` and `chunked.rs` are byte-identical to the adopted Handy
  baseline `73c09585`; the latter two are intentionally not advanced to the newer
  automatic-language-planning pipeline. The capability/VAD/live startup decision
  follows current Handy directly. Catalogue JSON exactly matches Handy `82dfab9f`.
- Upstream ratchet passes after tightening actual shared-code budgets: actions
  848 to 798; commands/mod 21 to 20; model commands 78 to 66; model manager
  228 to 222; tray 659 to 653; composition root 1341 to 1338. The only new
  two-line entry is the reviewed, exact upstream catalogue metadata update.
- The UI detector reports no findings. No Computer Use, new harness, installer
  or native engine upgrade was used.

The remaining shared-code differences are existing Grain features (Flow,
AI/Prompt Record, capture admission, events, Paste Catch and presentation),
not another standard/streaming implementation. The presentation adapter takes
one preview flag: Handy dictation supplies model capability; Agent and Flow
supply false. Agent still loads/transcribes `selected_model` through the same
existing manager, without another routing/model-selection layer.

### User acceptance (not yet marked tested)

Exit other Grain instances, then run:

```powershell
cd C:\Projects\Grain\grain-dictation-unified-20261106
$env:CARGO_TARGET_DIR = 'C:\gtc'
bun run tauri dev
```

1. Catalogue: one model list/selection; search, language ranking and downloads
   work. There is one Dictation shortcut and no Standard/Streaming model slots.
   Confirm the catalogue layout visually in light/dark mode.
2. Choose a streaming model (e.g. Parakeet Unified English). Use the Dictation
   shortcut, speak, stop and repeat. Live preview follows the pill preference;
   text pastes and audio/history are saved through Handy's shared path.
3. Choose a batch model, then reviewed Parakeet TDT v2/v3. Use that same shortcut
   for each. Batch follows Handy; TDT uses existing Flow. Check cancel/repeat.
4. With a streaming model selected and Live preview enabled, summon Agent.
   It uses that selected model and keeps its compact voice-input pill.
5. Check optional AI finish and Prompt Record once with a streaming model, then
   once with Flow. No separate AI capture-model setting should appear.
6. For first-run layout, `bun run dev:onboarding` exercises the real setup path:
   one model choice, one dictation shortcut, durable completion. This is a manual
   check; do not replace the live app with a mock UI.

The implementation is committed on the isolated convergence branch. Main is
unchanged; no installer/release was started. Device/visual acceptance above
remains open before any requested merge.
