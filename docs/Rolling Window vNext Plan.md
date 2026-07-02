# Rolling Window vNext — Analysis, Research, and Plan

> Status: **PLAN ONLY — no code changes yet** (owner directive). Written after a
> full read of `crates/rolling-window` + `src-tauri/src/rolling.rs` and web
> research on current streaming-ASR practice. Companion: `docs/TRANSITION-LOG.md`
> (the unified transcribe-cpp engine this plan builds on).

---

## 1. Current architecture (post-unification, July 2026)

```
mic frame (16 kHz f32, high-passed)
  └─ AudioRecorder sample callback
       └─ RollingTranscriber::feed          (Mutex<Option<Arc<RollingSession>>>)
            └─ SessionCursor::push_block    (f32→i16, raw-RMS silence tracking)
                 ├─ hard cut: ≥ max_chunk_seconds (user 15–60 s) unsent
                 └─ early cut: ≥ 0.6 s silence after ≥ 10 s unsent
                      └─ AudioChunk { samples, start/fresh_start/end (abs sec) }
                           └─ mpsc → session worker (serial)
                                ├─ optional boost-only AGC (per chunk)
                                ├─ TranscriptionManager::transcribe(chunk)   ← shared engine, rolling_hold
                                └─ TimelineAssembler::add_chunk(text, words=None)
                                     └─ merge_transcript (plain text fallback!)
stop → cursor.stop() tail → worker drains → assembler.text() → finalize once → paste
```

Sound parts worth keeping (verified by the ported test suite):
- **Absolute-frame cursor** — the timeline is ground truth we own; the tail can
  never be dropped; buffer is compacted to ~one chunk + overlap (bounded RAM,
  ~1–2 MB i16 regardless of session length). This is genuinely good design.
- **Timeline-tagged chunks** (`start/fresh_start/end`) — dedup can be arithmetic
  instead of text guesswork *when word timings exist*.
- **Serial worker + engine share** — no second model in RAM, no concurrency on
  the engine, audio thread never blocks on inference.

---

## 2. Defects, regressions, and debt found (ranked)

**R1 — REGRESSION (high): the preferred dedup path is dead.**
`rolling.rs` now calls `assembler.add_chunk(..., words = None)` because
`TranscriptionManager::transcribe` returns only `String`. With `words=None` the
assembler bails to `merge_transcript` — the *plain text* longest-suffix/prefix
merge — and **neither** the time-based dedup **nor** the fuzzy seam pass
(`with_fuzzy_seam` is only consulted on the timed path) ever runs. Consequences:
a genuinely repeated phrase at a boundary gets over-stripped ("yes yes" → "yes"),
and an overlap the model re-words differently ("won too tree" vs "one two three")
gets duplicated. The old grain-transcribe path synthesized timings precisely to
avoid this; the unification silently lost it.

**R2 — BUG (medium): zero-fresh tail chunk can duplicate the transcript tail.**
`SessionCursor::stop()` with nothing unsent still emits the 2 s overlap window
(`fresh_start == end`). The timed path has an explicit zero-fresh guard; the
text path (the only one running today, see R1) has none — `merge_transcript`
relies on the model re-transcribing the overlap *identically* to dedup it. A
re-worded overlap appends garbage at the very end of the paste.

**R3 — DEBT: `ChunkPump` is exported but unused.** Its contract (readiness
gate, single-flight, stale-generation discard) is currently *implicitly*
provided by the TM loading condvar + the serial mpsc worker. Per "destroy if
not in use": delete it, or make it the explicit dispatch spine (see P5.1).

**R4 — PERF (minor): per-frame `Mutex` on the audio thread.**
`RollingTranscriber::feed` locks `active` for every captured frame and
`RollingSession::feed` locks `cursor`. Uncontended in practice, but a
`finish_session` can stall a capture callback. Cheap fix exists (P1.4).

**R5 — QUALITY: silence detection is a fixed raw-RMS threshold (0.008).**
Noisy rooms / fans defeat the early-finalize entirely (everything becomes hard
15 s cuts mid-word); very quiet mics can trip it during soft speech. Meanwhile
the recorder ALREADY runs Silero VAD (smoothed) on the same frames — that
signal is thrown away for chunking purposes.

**R6 — QUALITY: hard cuts land at arbitrary samples.** When no silence is
found, the cut happens mid-phoneme. WhisperX showed boundaries snapped to
speech-activity minima measurably reduce boundary errors and hallucinations.

**R7 — QUALITY: no cross-chunk context.** Each chunk decodes cold. Whisper-
family models accept `initial_prompt` (we already plumb it for custom words);
conditioning on the committed tail improves boundary coherence, spelling
consistency, and casing across chunks. (Known hazard from the literature:
conditioning on *unstable* text propagates errors — only committed text, and
only where `Feature::InitialPrompt` is advertised.)

**R8 — LATENCY (future live preview): 10–17 s to first text.** Fine for
paste-at-stop, useless for the owner's planned rolling live preview.

**R9 — NITS:** `AudioChunk.attempts` is dead (no retry exists);
`OVERLAP_SEARCH_WORDS=30` text-merge window can eat a legitimately repeated
long phrase (only matters while R1 persists); `overlap_seconds = 2.0` is
conservative once real timings return (re-transcribing 2 s per ~13 s chunk is
~15% wasted compute).

---

## 3. Research digest — where the bleeding edge is

- **LocalAgreement-n** (ufal/whisper_streaming, "Turning Whisper into a
  Real-Time Transcription System", ~3.3 s latency): re-decode a growing buffer;
  commit the longest prefix on which n=2 consecutive hypotheses agree; trim the
  buffer at sentence/segment ends; carry committed text forward as init prompt.
  → Directly applicable to our *live preview* phase; the commit rule is
  engine-agnostic (pure text comparison), so it stays model-agnostic.
- **AlignAtt / SimulStreaming** (2025 SOTA, ufal/SimulStreaming, WhisperLiveKit
  default): stop decoding when cross-attention approaches the audio frontier.
  Requires decoder-internal attention access. **Not applicable to us**:
  transcribe-cpp's batch API doesn't expose attention, and for models that
  genuinely stream, transcribe-cpp's own `session.stream()` already implements
  an internal commit policy — that's the Native ASR path, not Rolling's job.
- **WhisperX VAD cut & merge** (Interspeech 2023, adopted by faster-whisper
  batching): chunk boundaries chosen at speech-activity minima ("min-cut"),
  chunks packed toward the model's sweet spot; removes timestamp-token
  dependence; measurably fewer repetition/hallucination artifacts.
  → Directly applicable to `SessionCursor` (R5/R6).
- **Context carry-over**: `initial_prompt`/previous-text conditioning is
  standard (SpeechBrain, faster-whisper); qwen-asr streams with a
  *rollback-suffix* (drop the last ~5 tokens of the prior output before
  conditioning) to stabilize boundaries; `condition_on_previous_text=False`
  when hallucination-propagation risk outweighs coherence.
  → Applicable as P3.2, committed-text-only.
- **Confidence-aware merging**: modern pipelines resolve overlap disagreement
  by decoder confidence instead of "previous chunk wins".
  → transcribe-cpp exposes exactly what this needs (below).

**Decisive local finding:** `transcribe_cpp::Transcript` returns `segments`,
`words` (ms-accurate), and `tokens` **with a per-token confidence `p`**, gated
by `RunOptions.timestamps: TimestampKind::{None,Auto,Segment,Word,Token}`, and
each model advertises `max_timestamp_kind` (catalog: parakeet/nemotron = token,
whisper = segment+). So the assembler's preferred timed path can run on REAL
word timings for essentially every catalog model — strictly better than the
synthesized timings the old grain-transcribe path used.

---

## 4. The plan

Ordering favors correctness first, then boundary quality, then merge quality,
then the live preview. Every phase is independently shippable and keeps the
invariants: **one engine in RAM, bounded buffers, model-agnostic, rolling code
isolated from ported upstream files** (the only upstream touch is one
`[GRAIN]`-marked accessor in `transcription.rs`).

### Phase 1 — Restore the timed dedup path (fixes R1, R2, R4)
1. **P1.1** `[GRAIN]` add `TranscriptionManager::transcribe_with_timings(audio)
   -> Result<(String, Vec<WordTiming>)>`: same engine-take/catch_unwind path as
   `transcribe`, but requests `TimestampKind::Word` (fall back `Auto`) and maps
   `Transcript.words` (ms, audio-relative) → chunk-relative `WordTiming` secs.
   Rolling-hold semantics identical. (Alternative considered and rejected:
   returning `Transcript` from `transcribe()` — bigger upstream diff.)
2. **P1.2** `rolling.rs`: call `transcribe_with_timings`; when `words` is
   non-empty pass it to `add_chunk` (fuzzy seam stays on as the safety net for
   timing jitter); when a model returns none, **synthesize** evenly-spaced
   timings across the chunk duration (restores the old behavior; the fuzzy
   seam was designed for exactly this) so `merge_transcript` becomes truly
   last-resort.
3. **P1.3** Skip enqueuing chunks with `fresh_duration_sec() == 0` in the
   driver (closes R2 for every path; the assembler's own guard stays as
   defense in depth).
4. **P1.4** Replace `Mutex<Option<Arc<RollingSession>>>` with
   `arc_swap::ArcSwapOption<RollingSession>` (or `parking_lot` try_lock +
   drop-frame) so the audio callback never blocks on session teardown.
5. **Tests**: port the "differently-transcribed overlap" and "repeated phrase"
   scenarios through the real driver path (mock TM via a trait or feed the
   assembler directly with word lists from a fixture); regression test for the
   zero-fresh stop flush in text mode.

### Phase 2 — VAD-aware, snap-to-minimum boundaries (fixes R5, R6; WhisperX idea)
1. **P2.1** Extend the recorder's fan-out to hand rolling a per-frame
   *speech-probability* (the SmoothedVad decision it already computes) beside
   the samples; `SessionCursor` gains `push_block_with_vad(block, speech: bool)`
   — early-finalize triggers on VAD-silence runs instead of the raw-RMS
   threshold (keep RMS as fallback when VAD is disabled).
2. **P2.2** Hard-cut snapping: when the hard cut fires, scan the last
   `snap_window` (default 3 s) of retained audio for the longest VAD-quiet run
   (fallback: minimum 200 ms RMS energy window) and cut THERE instead of at the
   arbitrary current frame. The cursor already retains this audio; zero extra
   memory. Boundary lands between words like WhisperX's min-cut.
3. **P2.3** Retune defaults with real timings in place: `overlap_seconds`
   2.0 → 1.0 (research: 1 s suffices for alignment-based dedup; ~8% less
   recompute), `early_min_seconds` 10 → 8. Keep the user's 15–60 s hard max.
4. **Tests**: synthetic sessions with known VAD tracks asserting cut positions;
   bound test unchanged (buffer stays ≤ chunk+overlap).

### Phase 3 — Smarter seam + context (fixes R7, upgrades merge quality)
1. **P3.1** Confidence-weighted overlap resolution: request
   `TimestampKind::Token` where `max_timestamp_kind` allows; in the overlap
   region `[fresh_start − overlap, fresh_start + tolerance]` both chunks
   transcribed the same audio — when their word sequences *disagree* (fuzzy
   alignment already computes this), keep the version with higher mean token
   `p` instead of always previous-chunk-wins. Model-agnostic: models without
   token confidence (p = NaN) keep today's behavior.
2. **P3.2** Committed-context conditioning (whisper-family only): pass
   `initial_prompt = custom_words ++ tail(committed_text, ~200 chars)` for
   rolling chunks, minus a ~5-word rollback suffix (qwen-asr's boundary
   stabilizer). Strictly committed text only — never the current chunk's own
   output — so error propagation is bounded; gated behind a setting default ON
   for whisper, OFF otherwise (`Feature::InitialPrompt` check already exists).
3. **Tests**: seam disagreement fixtures with confidence tables; prompt
   contents asserted via a capture shim.

### Phase 4 — Rolling live preview (owner's stated later goal; uses R8 research)
1. **P4.1** Cheap committed preview (near-free): after each chunk merges, emit
   the assembler's committed text to the pill as `AsrStreamText { committed }`
   (the Studio Window path already renders this). Latency = chunk cadence
   (~8–15 s). Toggleable setting; OFF keeps today's behavior.
2. **P4.2** LocalAgreement-2 tail preview (optional, compute-gated): between
   chunk boundaries, when the worker is idle, decode the unsent tail
   (cursor-slice, no copy growth) every ~3 s and run LocalAgreement-2 against
   the previous tail hypothesis; agreed prefix → `tentative` for the pill.
   Pure text comparison → model-agnostic. Costs one extra decode of ≤15 s
   audio every ~3 s — enable only on GPU backends and only while the preview
   toggle is on (low-RAM/low-CPU ethos: this is strictly opt-in).
3. Explicit non-goal: AlignAtt/SimulStreaming-style decoder policies (no
   attention access through transcribe-cpp; streaming-capable models already
   have the Native ASR path for true streaming).

### Phase 5 — Housekeeping
1. **P5.1** Delete `chunk_pump.rs` (contract is enforced by the TM condvar +
   serial worker; the ported spec lives in git history). If Phase 4.2 ever
   needs cancellable dispatch, resurrect it then — not before.
2. **P5.2** Drop dead `AudioChunk.attempts`, or implement the one thing it was
   for: a single retry on transient engine error (recommend: drop).
3. **P5.3** Doc pass on the crate header (it still references the Python port
   lineage and "preferred when timings available" — after P1 that's simply
   "the path").

### RAM / compute budget (unchanged or better at every phase)
| Component | Today | After plan |
|---|---|---|
| Engines resident | 1 (shared) | 1 (shared) |
| Cursor buffer | ≤ chunk+overlap (~2 MB @60 s) | same; −overlap shrink in P2.3 |
| Assembler | O(transcript text) | same (+ tail-hypothesis string in P4.2) |
| Extra decode work | — | P4.2 only, opt-in, GPU-gated |
| Word timings | none | `Vec<Word>` per in-flight chunk, freed on merge |

### Sources
- [ufal/whisper_streaming — LocalAgreement, buffer trimming, prompt carry-over](https://github.com/ufal/whisper_streaming)
- [ufal/SimulStreaming — AlignAtt policy (2025 successor)](https://github.com/ufal/SimulStreaming)
- [WhisperLiveKit — production local streaming stack (SimulStreaming default, Silero VAD)](https://github.com/QuentinFuxa/WhisperLiveKit)
- [WhisperX — VAD cut & merge, min-cut boundaries, forced alignment](https://arxiv.org/abs/2303.00747)
- [CUNI IWSLT 2025 — offline models in simultaneous mode, AlignAtt vs LocalAgreement ranking](https://arxiv.org/html/2506.17077)
- [qwen-asr — rollback-suffix prompt conditioning for chunk boundaries](https://github.com/antirez/qwen-asr)
- [DCTX-Conformer — dynamic context carry-over for streaming ASR](https://arxiv.org/pdf/2306.08175)
- transcribe-cpp 0.1.0 vendored source (`result.rs`: `Transcript{segments,words,tokens(p)}`, `types.rs`: `TimestampKind`, `model.rs`: `max_timestamp_kind`)
