# Local transcription contract

Cloud speech-to-text support was removed on 2026-10-04. Speech audio is decoded
on-device. AI text processing and Agent LLM providers remain separate features;
network access is still used for those features and model downloads.

## Removal plan and scope

1. Remove HTTP audio adapters, the speech provider router and its quota tracker,
   provider commands, persisted settings and credentials.
2. Route batch callers through one local adapter. Keep shared engine ownership,
   loading, inference and text cleanup in Handy's transcription manager.
3. Remove cloud settings forms, the singleton store and initialization, search
   terms, status indicators, translations and generated command/types.
4. Remove unused multipart uploads and the unconsumed legacy provider router;
   retain the then-current LLM implementation (subsequently simplified in
   [LLM-FALLBACK.md](LLM-FALLBACK.md)).
5. Verify persistence/routing, build and test the application, audit independently,
   update review routes and tighten measured shared-code budgets.

## Owned seam

`src-tauri/src/grain_transcription.rs` contains the former local batch branch:
idempotent loading, then `spawn_blocking` around the shared manager's
`transcribe`. It has no HTTP client, settings pool, timeout, quota or state.
Prompt Record, Agent voice input and extension/action captures use this same
manager. Their microphone readiness, feedback and cleanup are retained.

Standard (`transcribe`, Alt+Space) selects reviewed installed Parakeet TDT v2/v3
Flow through `grain-core/capture.rs` and `grain_dictation_routing.rs`. Other
models use ordinary local batch inference. Translate to English retains the
batch path. Streaming (`transcribe_native_asr`, Ctrl+Space) retains its separate
model selection and native streaming path.

Speech provider types, commands, cloud toggles and STT credential maps no longer
exist in the product contract. Unknown development settings are ignored and
disappear on the next save; unrelated LLM/extension credentials survive. No
cloud fallback or hidden opt-in remains.

## Upstream review

`relocations.json` includes the local adapter under transcription-manager
review, and no longer points at the deleted HTTP speech client. Review changes
to shared loading, engine checkout, cancellation and finalization against this
adapter and the retained Flow/Streaming callers. Preserve the resampler tail
drain: it protects complete local audio even though its historical motivation
was cloud decoding. Historical upstream verdicts and ledgers remain evidence
of the code that existed when those ports were assessed.

## Verification

The retirement regression loads obsolete cloud settings and credentials,
checks that the Parakeet model still selects local Flow, then saves and verifies
that retired data is omitted while LLM and extension credentials are preserved.
Run grain-core tests and the current LLM fallback tests, targeted recorder/coordinator/overlay/LLM
tests, Specta export, frontend build/lint/tests, settings parity and upstream
policy/preflight. Independent review checks runtime reachability, retained
local behavior, credentials, dependencies and current maintenance records.

Real-app acceptance remains a user check: Transcription should show the two
local model slots and engine setting without cloud controls; Standard,
Streaming, Agent voice and AI text processing should still work.

### Recorded results (2026-10-04)

- 208 existing grain-core tests and the new retirement regression pass; all
  10 retained provider-router rotation tests pass.
- 47 targeted Tauri tests pass: recorder 14, coordinator 8, overlay 9,
  shortcut conflicts 3, Agent 5, LLM client 6 and post-processing 2.
- Specta export, TypeScript/Vite production build, lint, 116 frontend tests,
  settings parity (85 fields), formatting and upstream preflight pass.
- Independent read-only audit found no actionable correctness/security or
  regression findings. Its stale provider-pool comment was cleaned up.
- Shared-code costs decrease: Cargo.toml 141→139, actions.rs 654→641,
  commands/mod.rs 23→22 and lib.rs 1266→1260 (22 lines total). The deleted
  Grain-only adapters/forms/router are outside that shared-code metric.

Rust verification ran on Windows. A command-local Tauri resource override
avoided copying DLLs locked by the user's running Grain; production config was
unchanged. Real-device audio/model acceptance and other operating systems have
not been exercised by this audit.
