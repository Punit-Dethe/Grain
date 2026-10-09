# App main integration checkpoint

**10 October 2026.** This block integrates existing product work; it adds no
extension feature, authentication framework or acceptance scenario.

## Source and ownership

- Integration branch: `extensions/main-integration-20261010`.
- Extension starting commit: `cdc0a4ac93637155df31cc3c1883e4ef63cff61d`.
- App main integrated: `bbae21ccadc4c6e8d65263e207d644db41ab74ad` (version 0.0.8).
- Shared ancestor: `076c8d7918c99f950a2ca6af0cd5248ed9045bbd`.
- Safety checkpoint branch: `extensions/checkpoint-before-main-20261010`.
- Isolated checkout: `C:\Projects\Grain\grain-main-integration-20261010`.
- Published merge: `4556eae5110b15bfac1ad868ebda08d977b01194`.
- Post-merge ancestry: **149 ahead / 0 behind** the main snapshot above.
- Integrated tester alpha build:
  [37977984049](https://github.com/Punit-Dethe/Grain/actions/runs/37977984049),
  dispatched for that merge. **In progress** at handoff; no installer success
  is claimed yet. Only the Windows alpha job runs; the normal matrix is skipped.

The original `C:\Projects\Grain\grain` checkout and its preexisting local changes
are preserved. Continue integrated development in the integration checkout;
the old extension branch is not silently rewritten or reset. Main itself is not
changed by this integration.

The repository's `merge=ours` attributes protect Grain-owned files during Handy
syncs, but also apply to merges from Grain's own main. The first attempt revealed
that these attributes silently retained old frontend settings. That attempt was
aborted and repeated with the documented real three-way merge driver. The final
result includes main's actual frontend, settings and workflow changes.

## What was reconciled

| Boundary | Result |
| --- | --- |
| Recording and onboarding | Main's internal WebView overlay, local transcription, capture readiness and shorter onboarding replace the older pill process. Deleted main modules are not restored just to satisfy references. |
| Agent | Main's selected-text/input and provider-fallback behavior coexist with the extension branch's tool loop, run cancellation, confirmation ownership, outcome receipts and transient-shortcut guards. Removed field-context preferences stay removed. |
| Core settings | Dedicated Agent/Snippets commands remain registered and used by the UI. Main's overlay controls and structured-error handling are retained. |
| Native transport | Worker/developer authentication, bounded requests/writes and connection-owned cleanup remain. The retired Pill role, full-trust token, reverse channel and core-event subscription are gone. Internal UI events reach the WebView bridge. |
| Extension settings | Retired contribution rendering remains inactive; main's older contribution controls are not resurrected. |
| MCP and publication | Existing native/MCP contract, custom connections, store controls, rmcp/oauth2 reuse, exact GitHub credential scope and alpha trust identity remain. No catalogue or registration is republished. |
| Generated files | Bindings are regenerated from final Tauri command registration. Both Cargo locks are reconciled offline against final manifests without a blanket dependency upgrade. |

Source review compared the overlapping command, settings, Agent, socket and
SDK files against both parents. Authentication/protocol code is reused, not
reimplemented. Upstream Handy changes come from app main; no new features are
placed inside the Handy-derived tree.

## Small fixes exposed by a fresh checkout

- The signed revocation fixture had committed LF bytes while its signed index
  expected the exact CRLF seed bytes. The original checkout contained the matching
  bytes, masking the defect. Copying the exact seed bytes repairs the fixture;
  its existing public test signature already matches those bytes. Production
  signatures, keys, generation checks and expiry policy are unchanged.
- The existing harness fingerprint no longer requires an untracked local
  `package-lock.json`; the tracked `bun.lock` remains required. An npm lock is
  included when present. The new recording HTML and embedded seed are fingerprinted,
  and the stamp command creates its own output directory on a fresh checkout.
- Harness setup drops fields removed by main and disables main's provider fallback
  for the controlled model. No runner, peer, scenario or alternative UI is added.
- The existing UI parity parser accepts indentation in regenerated command bindings.
  Its setting-reachability rules are unchanged.

Three tests for the removed event-filter API were removed with that API. They
are not replaced by a test-only always-false filter. Actual token authentication,
revocation, socket cleanup and retired host-API checks remain.

## Verification

- Frontend: 116 unit tests pass; TypeScript and production build pass.
- Rust workspace: full workspace tests pass; final locks resolve in locked mode.
- Normal Windows backend: **720 Pass, 0 Fail, 3 intentionally ignored**; explicit
  binding export also passes. Ignored live-catalogue/model checks are not claimed
  executed. The full regression was rerun after the fixture repair.
- Existing runner/client-metadata self-tests: 81 pass.
- UI parity and upstream boundary policy checks pass.
- Native fork verification: 10 tests pass; process-guard tests: 16 pass.
- Existing real-app suites: **24/24 Pass**, with independent owned cleanup Pass
  for every suite. These use real Grain/WebView/SDK paths and controlled peers/model;
  they do not certify the configured genuine model or live GitHub service.

| Suite | Cases | Retained local report |
| --- | --- | --- |
| `extension-contract` | 12 Pass | `tests/agent-harness/.runs/run-MWGT2o/evidence/report.md` |
| `agent-workflow` | 6 Pass | `tests/agent-harness/.runs/run-FLUDAh/evidence/report.md` |
| `mcp-configured` | 3 Pass | `tests/agent-harness/.runs/run-Rv6UeD/evidence/report.md` |
| `store-mcp` | 3 Pass | `tests/agent-harness/.runs/run-6CQlwG/evidence/report.md` |

Reports identify the pre-merge HEAD and the dirty source/build fingerprints;
they were run before the merge commit. Final whitespace formatting and raw
binding regeneration were followed by a fresh TypeScript/production build check.
Generated reports, profiles and build
outputs remain ignored disposable evidence. Do not commit or grow that material.

## Installed-alpha acceptance still required

The previously installed 0.0.6 alpha is not the integrated 0.0.8 build. After
the existing isolated alpha workflow produces a successful installer:

1. Enable Agent through Settings, invoke its configured shortcut, submit a
   simple instruction and close the reply. Verify enablement after restart.
2. Dictate a short sentence into an editor using the usual dictation shortcut.
   Confirm the new recording overlay appears and the text lands correctly.
3. Check Extensions: GitHub appears in the store/Installed area and your own
   MCP connections remain alongside installed extensions. Sign in if required;
   ask Agent for a harmless read from `Punit-Dethe/grain-github-alpha-test`.
4. Approve one disposable issue creation in that test repository, verify the
   issue exists once, then check restart/read and disconnect/reconnect. Keep
   receipts; never automatically replay an uncertain write.

These are Pending until observed on the integrated installed alpha. Earlier
user-tested Agent opening and GitHub install/sign-in remain historical Pass,
not acceptance of this new build. Real issued Linear-token expiry remains
explicitly Deferred. Physical shortcut/dictation and visual acceptance are not
inferred from tool or controlled-model tests.

After this installed checkpoint, return to the next small product block in the
[execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md): provider/publisher onboarding
and any concrete usability gaps. Do not restart historical phase/test inventories.
