# Offline Flow accuracy audit

Compares the exact same local WAV through:

1. Ordinary transcribe.cpp Batch inference.
2. Current Flow inference using production `grain-tdt` window geometry and token merging.
3. A diagnostic using ordinary inference on the same windows, with optional extra right context. Context tokens are filtered **after** decoding; this is not a production bounded-decoder implementation.

This standalone tool is excluded from application builds. It loads one model, runs sequentially, and releases it on exit. Batch evaluation needs the entire WAV in memory; use bounded evaluation clips rather than hour-long recordings. No microphone, Tauri, LLM, dictionary, VAD or Grain gain normalization is involved. PCM16 history recordings have already been quantized; they do not recreate the original Float32 journal bit-for-bit.

## Run

Requires a matching Grain-patched native 0.2.3 library with `PKFW` support, its backend modules, and a reviewed Parakeet TDT v2/v3 GGUF. Models and recordings are not downloaded or uploaded by this tool. The audit used v2 Q8_0, CPU, four native threads, and no language hint on any path.

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

Without ground truth, edit counts measure **disagreement with Batch**, not word error rate. The word comparison ignores case and punctuation at word edges; it does not implement language-specific tokenization. Exact transcripts remain available for punctuation, casing and multilingual inspection. Timings measure total compute, not live recording stop latency; CPU warmup and execution order affect comparisons.

## Verify

```powershell
cargo test --manifest-path scripts/flow-audit/Cargo.toml --offline
cargo clippy --manifest-path scripts/flow-audit/Cargo.toml --offline --all-targets -- -D warnings
cargo fmt --manifest-path scripts/flow-audit/Cargo.toml -- --check
```

See [the audit and implementation plan](../../docs/FLOW-ACCURACY-AUDIT.md).
