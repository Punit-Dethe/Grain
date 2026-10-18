# Flow conformance audit

Runs the production policy for reviewed Parakeet TDT v2/v3 GGUF models: native
stateful buffered decoding (125 left / 125 owned / 62 right encoder frames).
The former Fluid window, padding and overlap merger are removed. No parameter
sweep, pause-cut heuristic or independent-window text assembly remains.

This tool is excluded from the application. It loads one model, runs serially,
and releases it on exit. It reads the complete WAV into memory for offline
inspection; use bounded public clips. The application uses its bounded journal
reader instead. Models and audio are never downloaded or uploaded here.

Both Cargo workspaces pin the same immutable native source pair. Verify with:

```powershell
uv run --no-project scripts/transcribe_fork.py --check --metadata --official
cargo run --locked --manifest-path scripts/flow-audit/Cargo.toml -- 'C:/path/model.gguf' 'C:/path/native-bin' 'C:/path/report.json' 'C:/path/public.wav'
```

Inputs must be mono 16 kHz PCM16 or Float32 WAVs. Additional WAV arguments are
processed serially. The explicit output file contains transcripts, owned token
sequences and compute timings. Stdout contains window counts only.

Optional environment variables:

- `FLOW_AUDIT_BACKEND`: `cpu` (default), `vulkan`, `cuda`, `metal` or `auto`.
  The requested backend must exist in the compiled library; the report records
  the actual backend.
- `FLOW_AUDIT_THREADS`: CPU thread count (default 4). Use `0` for the native
  automatic default, matching the real worker. Compare runs with identical
  thread settings; quantized numerical differences can change predicted text.
- `FLOW_AUDIT_REFERENCE`: ground-truth text file for exactly one WAV. WER uses
  whitespace words with case and edge punctuation ignored. Full text remains
  available for separate casing/punctuation inspection.

Compute timings exclude capture, cold loading and worker transport. They are
not live Stop latency. A few public regression clips do not establish general
WER or an optimal context size. The selected v2 policy comes from published
NVIDIA results, not private-microphone tuning. See
[v2 evidence](../../docs/FLOW-V2-STATEFUL-PLAN.md) and
[v3 evidence and limits](../../docs/FLOW-V3-STATEFUL-PLAN.md).

```powershell
cargo test --locked --manifest-path scripts/flow-audit/Cargo.toml
cargo clippy --locked --manifest-path scripts/flow-audit/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path scripts/flow-audit/Cargo.toml -- --check
```
