# Fixes Grain carries that Handy doesn't — candidates for upstream PRs

Every item below is a **bug fix or clear improvement inside code Handy owns**,
carries no Grain-specific dependency, and would benefit any Handy user. Sending
these upstream is the cheapest possible isolation work: each one Handy accepts
deletes a permanent line-item from
[UPSTREAM-DIVERGENCE.md](../Upstream%20Tracking/UPSTREAM-DIVERGENCE.md) forever,
instead of being re-merged by hand at every sync.

**Verified against upstream `cdbc2239` (2026-07-20, latest `main`, ahead of
tag v0.9.3).** The four commits Handy has since our merge base are all
frontend/i18n — none of them touch the Rust files below, so every fix here is
still absent upstream. Re-verify before opening a PR if time has passed:

```bash
git fetch upstream --tags
git show upstream/main:<path> | grep -n "<marker>"   # markers listed per item
```

Suggested PR order: 1 and 2 are self-contained and easy to review, so lead with
those.

---

## 1. Resampler drops the tail of every recording

- **File:** `src-tauri/src/audio_toolkit/audio/resampler.rs` (`FrameResampler::finish`)
- **Marker to re-check:** `output_delay`
- **Severity:** user-visible data loss

`FftFixedIn` has an internal latency of `output_delay()` output frames: early
`process()` calls emit fewer frames than the steady-state rate while the FFT
delay line fills. At end-of-stream that many output frames are still trapped
inside the resampler. Upstream's `finish()` processes the leftover partial
input but never drains the delay line, so the last `output_delay()` frames —
the final fraction of a second of the recording — are silently discarded.

Local ASR usually tolerates the clipped tail. Cloud STT transcribes exactly the
bytes it is sent, so the final word gets cut.

**Fix:** after processing the zero-padded remainder, feed silent full-size
chunks until at least `output_delay()` output frames have been pulled out.

> Note for the PR: Grain's version of this function also differs cosmetically
> (`emit_frames_into` instead of `self.emit_frames`, to satisfy the borrow
> checker after the change). Port the fix, not Grain's helper split, unless the
> reviewer prefers it.

---

## 2. Deleting a recording intermittently fails on Windows

- **File:** `src-tauri/src/managers/history.rs`
- **Marker to re-check:** `remove_file_with_retry`
- **Severity:** intermittent user-visible failure, Windows-only

A single `fs::remove_file` on a just-finished recording (or one being previewed
in the history UI, or one a virus scanner still holds) intermittently returns
`ERROR_ACCESS_DENIED` (os error 5) even though the handle is about to be
released. The delete fails and the user sees an error for a file that is
perfectly deletable a moment later.

**Fix:** `remove_file_with_retry` — up to 5 attempts with a short backoff;
`NotFound` counts as success.

---

## 3. Per-chunk allocation on the realtime audio callback

- **File:** `src-tauri/src/audio_toolkit/audio/recorder.rs` (`build_stream`)
- **Marker to re-check:** `output_buffer.clone()` (upstream still has it)
- **Severity:** performance, hot path

The cpal input callback sends each captured chunk with
`sample_tx.send(AudioChunk::Samples(output_buffer.clone()))`. That is one heap
allocation plus a memcpy **per audio chunk, for the entire recording**, inside
the realtime callback.

**Fix:** `std::mem::replace(&mut output_buffer, Vec::with_capacity(data.len() / channels))`
— hand the filled buffer to the consumer and leave a correctly-sized empty one
behind. Same semantics, no per-chunk copy.

---

## Also worth offering: extension points (not bug fixes)

These are the hooks that would let Grain — or any Handy fork/plugin — stop
patching Handy's files at all. Each is small and behaviour-preserving upstream.
Landing even one permanently removes a conflict source:

| Hook | File | What it enables |
|---|---|---|
| A raw per-frame sample callback (every resampled frame + its VAD decision), alongside the existing post-VAD `with_audio_callback` | `audio_toolkit/audio/recorder.rs` | Any consumer needing a continuous timeline rather than VAD-gated speech (Grain: rolling-window transcription). Grain's `with_sample_callback` is exactly this. |
| A post-transcript hook | `actions.rs` | Post-processing/routing without editing the action (Grain: voice actions, snippets, Prompt Record). |
| A typed event tap | `lib.rs` | Out-of-process UIs (Grain: the native pill). |

**The recorder sample callback is the highest-value one** — it is the single
hook that would let a fork implement live/rolling transcription without
touching `recorder.rs` at all.

---

## Deliberately NOT upstreamable

For contrast, so nobody wastes time offering these: the ONNX/transcribe-rs
removal (`managers/model.rs`, `managers/transcription.rs`), the multi-provider
STT/LLM routers, the native pill replacing the webview overlay, the grain-core
settings substrate, and every `grain_*` module. These are Grain product
decisions, not fixes — upstream should not want them.
