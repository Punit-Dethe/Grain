# Offline Flow accuracy audit

Compares the exact same local WAV through:

1. Ordinary transcribe.cpp Batch inference.
2. Historical Fluid-style Flow inference, retained as the regression baseline.
3. A diagnostic using ordinary inference on the same windows, with optional extra right context. Context tokens are filtered **after** decoding; this is not a production bounded-decoder implementation.
4. Independent ordinary native inference with a frame-aligned ceiling and optional quiet cuts. This shares the production v2 cursor and token merger; it does not filter context tokens.

This standalone tool is excluded from application builds. It loads one model, runs sequentially, and releases it on exit. Batch evaluation needs the entire WAV in memory; use bounded evaluation clips rather than hour-long recordings. No microphone, Tauri, LLM, dictionary, VAD or Grain gain normalization is involved. PCM16 history recordings have already been quantized; they do not recreate the original Float32 journal bit-for-bit.

## Run

Requires a matching Grain-patched native 0.2.3 library with `PKFW` support, its backend modules, and a reviewed Parakeet TDT v2/v3 GGUF. Models and recordings are not downloaded or uploaded by this tool. The audit used v2 Q8_0, CPU, four native threads, and no language hint on any path.

The audit and app now pin the same maintained native fork through independent
Git patch pairs/lockfiles. `uv run --no-project scripts/transcribe_fork.py --check
--metadata` verifies both. A normal Cargo build compiles that source; the
prebuilt-prefix command below is an explicit development override. Validate its
contract with `scripts/transcribe_fork.py --runtime-dir <native bin directory>`
before use; official source builds reject this override. See
`docs/TRANSCRIBE-CPP-FORK.md` for source, bootstrap and release gates.

Windows, with an existing native install prefix:

```powershell
cd C:\Projects\Grain\grain
$env:TRANSCRIBE_DIR = 'C:\gt\debug\build\transcribe-cpp-sys-148cfcb827963f2a\out'
$env:CARGO_TARGET_DIR = 'C:\gt'
$model = (Get-ChildItem "$env:USERPROFILE\.cache\huggingface\hub\models--handy-computer--parakeet-tdt-0.6b-v2-gguf\snapshots" -Recurse -File -Filter '*.gguf' | Select-Object -First 1).FullName
cargo run --manifest-path scripts/flow-audit/Cargo.toml --offline -- $model "$env:TRANSCRIBE_DIR\bin" "$env:TEMP\flow-audit.json" 'C:\path\recording.wav'
```

Pass additional WAV paths to evaluate multiple recordings. The tool requires 16 kHz mono PCM16 or Float32 input and at least one second per recording. Stdout contains summary counts; the requested JSON file contains transcripts, individual window token sequences and timings. Keep that report local unless deliberately sharing it. Completed recordings are saved to the report even if a later inference fails.

Optional diagnostic lookahead, from 0 to 4000 milliseconds:

```powershell
$env:FLOW_AUDIT_RIGHT_CONTEXT_MS = '2000'
# Run the same command with a different report filename.
Remove-Item Env:FLOW_AUDIT_RIGHT_CONTEXT_MS
```

Optional ground truth, with exactly one WAV:

```powershell
$env:FLOW_AUDIT_REFERENCE = 'C:\path\intended-transcript.txt'
# Run the same command.
Remove-Item Env:FLOW_AUDIT_REFERENCE
```

Window sweep, 15040..30000 milliseconds in 80 ms steps (default production ceiling, 24000):

```powershell
$env:FLOW_AUDIT_INDEPENDENT_MS = '24000'
$env:FLOW_AUDIT_PREFER_PAUSE = '1' # 720 ms quiet in the last 3.04 s
# Run with a separate report per ceiling.
Remove-Item Env:FLOW_AUDIT_INDEPENDENT_MS, Env:FLOW_AUDIT_PREFER_PAUSE
```

For isolated resource measurements, set `FLOW_AUDIT_RESOURCE_MODE` to `legacy`,
`candidate` or `batch`. This runs only that pipeline; compare transcripts to a
separately measured Batch report. Run each mode in its own process for meaningful
peak-memory comparisons. `FLOW_AUDIT_THREADS` accepts 1..8 (default 4).
`FLOW_AUDIT_BACKEND` accepts `cpu` (default), `vulkan` or `auto`; reports record the
actual loaded backend. Explicit Vulkan errors if unavailable. CPU with two
threads on this desktop is a constrained-thread check, not a laptop benchmark.
`FLOW_AUDIT_REPEATS=2` or `3` repeats the supplied fixtures in the same process
(resource mode only); each row records its `pass`. This separates first-use
shader/shape work from warm decodes without loading another model. Always pair
GPU candidates with Batch on the same GPU backend.

The app adapter also has an opt-in real-model journal regression test. Supply
`GRAIN_FLOW_TEST_MODEL`, `GRAIN_FLOW_TEST_NATIVE_DIR`, and
`GRAIN_FLOW_TEST_CASES` (path to a local JSON array of `{ "path": "WAV path",
"expected": "offline candidate text" }`). It exercises incremental appends and
fresh-accumulator replay using the actual production adapter and Float32 journal:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib real_model_journal_partition_and_fresh_replay_match_offline_policy -- --ignored
```

Use PCM16 mono 16 kHz fixtures for this test. No private fixtures or transcripts
are stored in the repository. On Windows, the lib-test executable may need the
Common Controls v6 manifest described in `docs/FLOW-TDT-REWRITE.md`.

Without ground truth, edit counts measure **disagreement with Batch**, not word error rate. The word comparison ignores case and punctuation at word edges; it does not implement language-specific tokenization. Exact transcripts remain available for punctuation, casing and multilingual inspection. Timings measure total compute, not live recording stop latency; CPU warmup and execution order affect comparisons.

## Verify

```powershell
cargo test --manifest-path scripts/flow-audit/Cargo.toml --offline
cargo clippy --manifest-path scripts/flow-audit/Cargo.toml --offline --all-targets -- -D warnings
cargo fmt --manifest-path scripts/flow-audit/Cargo.toml -- --check
```

See [the audit and implementation plan](../../docs/FLOW-ACCURACY-AUDIT.md).
