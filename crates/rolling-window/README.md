# Grain Rolling Window

Grain's Flow mode reduces finalization latency for long dictation by decoding
bounded windows while recording. It is separate from Native ASR streaming and
does not create a second model instance.

## Pipeline

```text
16 kHz recorder stream
  -> append-only temporary PCM16 journal
  -> bounded absolute-frame cursor
  -> bounded queue of frame-range descriptors
  -> one serial shared ASR worker
  -> timing-aware or lexical overlap reconciliation
  -> canonical lowercase/plain transcript
  -> one-time local finalization
  -> optional General/Email/Coding post-processing
```

The successful Flow path has one ASR decode per emitted chunk. If one range
fails, that exact range is retried once from the journal after normal serial
processing. An unrecoverable range fails explicitly; Grain never returns a
plausible transcript with a hidden audio gap.

## Memory and resource contract

- The cursor retains at most one active window plus overlap.
- The worker reuses one PCM/f32 window buffer.
- The channel holds at most eight fixed-size descriptors and owns no audio.
- Session audio lives in an auto-deleting PCM16 journal at about 1.92 MB/minute
  on disk instead of a duration-growing `Vec<f32>` in RAM.
- Flow does not retain the recorder's full-session batch buffer.
- One shared transcription engine is resident and all inference is serial.
- Cancellation waits for a non-preemptible current call, discards pending work,
  joins the worker, and releases the journal.

Transcript text necessarily grows with dictation length; retained audio,
descriptor storage, seam state, and telemetry do not.

## Boundaries and overlap

The default hard window is 25 seconds. VAD silence can finalize after enough
fresh voiced audio, while a forced full window snaps to a short low-energy gap
when possible. Silence-only spans advance the absolute timeline without an ASR
job and retain only 300 ms of speech pre-roll.

| Boundary | Next-window overlap |
|---|---:|
| Clean VAD silence | 1.0 s |
| Short quiet-gap snap | 1.5 s |
| Forced hard cut | 2.0 s |

No model-family branch changes this geometry. If decode real-time factor is at
or above 1.0, Flow cannot eliminate stop debt; select a faster/smaller model
instead of increasing overlap or adding parallel inference.

## Transcript assembly

Only native per-word timestamps are temporal evidence. Segment-only or
timestamp-free results use bounded fuzzy lexical suffix/prefix alignment. The
assembler keeps an eight-token provisional seam so the right-hand hearing can
repair a split word, name, or number. Older text is append-only, and stop commits
the escrow without another decode.

Raw Flow output is Unicode-lowercased and removes sentence/presentation
punctuation while preserving lexical structure inside URLs, email addresses,
paths, decimals, flags, versions, and code identifiers. Rolling never sends the
committed transcript back to the ASR model as dynamic prompt context. Optional
General, Email, or Coding post-processing owns final formatting.

## Telemetry

Session logs retain no time-series samples. They report:

- Per-chunk audio duration, decode duration, sample RTF, and EWMA RTF.
- Queued fresh-audio debt.
- Stop-time debt and stop-to-final duration.
- Peak retained cursor frames and worker-window frames.
- Peak queued descriptor count and fresh-audio debt.
- Final journal bytes, chunk count, and recovered-range count.

The metrics are constant-space atomics. They do not drive a scheduler or keep a
service alive after the session.

## Preview and non-goals

Live preview is opt-in and default-off. With preview off, the worker blocks on
the descriptor channel and performs no polling or tentative tail decode.
Preview redesign, Native ASR changes, attention/KV caching, model retraining,
parallel chunks, LLM seam repair, and whole-session in-memory fallback are not
part of this rolling architecture.

## Verification

Automated coverage includes long and silent synthetic sessions, absolute-frame
conservation, timestamp provenance, lexical seam escrow, bounded queue failure,
stalled fake decoding, stop-drain delivery, cancellation/join, journal cleanup,
range recovery, and canonical plain-text cases.

```powershell
cargo test -p rolling-window
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Release validation should compare stop-to-final logs on a fast and
near-real-time model, inspect peak RSS during a long dictation, test Bluetooth
shutdown buffering, and play a journal-backed history recording.
