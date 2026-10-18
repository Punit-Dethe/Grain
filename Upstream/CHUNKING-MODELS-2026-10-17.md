# Chunking and sentence continuity — 2026-10-17

## Scope and certainty

This is a source audit of Handy `9e57a0d7929fcc6417ea0d6e54c1b57f62635edd`
and native transcribe.cpp 0.4.0
`f1dcd15858c4bbba87ca97862b4e448b2cf15b8f`. The catalog contains **69 model
entries** (quantizations are files inside entries): **8 streaming**, **13
Whisper-family long-form**, and **48 expected app-chunked** entries.

The table below is expected routing from the pinned catalog and native loader
rules, with long-form chunking enabled and sufficiently long audio. It is not
a claim that all 69 models were downloaded, loaded or speech-tested. Loaded
`supports_streaming` and `Feature::LongForm` are authoritative; custom GGUFs
and changed metadata must be reconciled at runtime. In 0.4.0 the inspected
native long-form feature is set for Whisper and Parakeet models with a VAD
head. Parakeet Ultra is the documented VAD-head example, but is **not in this
Handy catalog**.

## What happens at a boundary

A pause is not necessarily the end of a sentence. For example, independently
decoding “I wanted to explain” and “why this matters” can produce
“I wanted to explain. Why this matters.” Choosing pauses can reduce this
problem but cannot prove a grammatical boundary.

Handy's app chunker targets pauses near 30 seconds, normally caps a chunk at
40 seconds, keeps clips up to 40 seconds whole, and waits for over 45 seconds
of buffered live audio before its first stable cut. It sends each chunk with
the **same run options**. It neither feeds the preceding transcript into the
next run nor carries its batch decoder state. The joiner trims, handles script
spacing and concatenates; it does not repair capitalization or sentence-ending
punctuation. Shared final-text cleanup is not a sentence-boundary model.

It is **not guaranteed** that every chunk begins with a capital or ends with a
full stop. That depends on the model, language, audio and decoding. However,
there is no seam-specific repair in this implementation, so the user's concern
is an open acceptance risk, not a solved property.

## Which context mechanisms actually exist

| Mechanism                       | Models / restrictions                                                                                                                                                                                                                                                                                 |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Native previous-token history   | Whisper's internal roughly 30-second windows can condition on preceding decoded tokens. It is controlled by `condition_on_prev_tokens`; temperature fallback can disable history for the next window. This is different from independent app-level calls, and does not guarantee perfect punctuation. |
| General context prompt          | Whisper and Qwen3-ASR advertise it. Qwen receives context in a system message, not a guaranteed transcript-continuation channel. Handy's current chunked decoder does not populate prior-chunk context.                                                                                               |
| Transcript prefix               | Whisper; Canary 180m-flash, 1b-flash, 1b-v2; Granite 4.1-2b-plus when its GGUF advertises it. The audio must contain the prefix's speech. It cannot safely be filled with the end of a disjoint previous chunk. Output/task restrictions also apply.                                                  |
| Vocabulary hints                | Dictionary terms, not sentence history. Vocabulary support cannot be used as evidence of cross-chunk context.                                                                                                                                                                                         |
| Native streaming state          | Trained streaming models use a continuous stream rather than these independent batch calls. State implementation and final punctuation remain model-specific.                                                                                                                                         |
| Offline Parakeet TDT            | TDT/RNNT has a predictor state inside a decode, but native one-shot TDT creates fresh LSTM state and start-token state for each call. No generic context/prefix feature is advertised for the catalog's offline Parakeet variants. TDT alone is not a continuity guarantee.                           |
| Grain Flow                      | v2 ordinary windows and v3 PKFW windows are isolated native utterances, with bounded audio context/overlap and token merging in Grain. These protect audio/token seams; they do not establish sentence-aware punctuation. Keep raw-case/punctuation acceptance tests for Flow too.                    |
| Native Parakeet Ultra long-form | VAD-head segmentation at up to 30 seconds; documented segments decode independently and are joined. Native long-form support alone therefore does not imply shared linguistic history. Not a current catalog entry.                                                                                   |

Granite prefix/vocabulary features are read from GGUF metadata; older files may
lack them. Canary's prefix gate depends on the loaded variant's prompt format
and tokenizer. Runtime feature checks remain mandatory.

## Complete catalog inventory

| Catalog model slug                       | Expected long-audio path    | Context/continuation surface                                                |
| ---------------------------------------- | --------------------------- | --------------------------------------------------------------------------- |
| `parakeet-unified-en-0.6b`               | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `nemotron-3.5-asr-streaming-0.6b`        | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `canary-180m-flash`                      | Handy VAD chunks (expected) | Transcript prefix only, with matching audio; not used across Handy chunks   |
| `cohere-transcribe-03-2026`              | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `whisper-medium`                         | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `Voxtral-Mini-4B-Realtime-2602`          | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `parakeet-tdt-0.6b-v3`                   | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-tdt-0.6b-v2`                   | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `Qwen3-ASR-0.6B`                         | Handy VAD chunks (expected) | Context prompt API; Handy does not pass prior chunk text                    |
| `Fun-ASR-MLT-Nano-2512`                  | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `canary-1b-flash`                        | Handy VAD chunks (expected) | Transcript prefix only, with matching audio; not used across Handy chunks   |
| `canary-1b-v2`                           | Handy VAD chunks (expected) | Transcript prefix only, with matching audio; not used across Handy chunks   |
| `canary-1b`                              | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `canary-qwen-2.5b`                       | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `cohere-transcribe-arabic-07-2026`       | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `Fun-ASR-Nano-2512`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `gigaam-v3-ctc`                          | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `gigaam-v3-e2e-ctc`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `gigaam-v3-rnnt`                         | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `gigaam-v3-e2e-rnnt`                     | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `granite-speech-4.1-2b-nar`              | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `granite-4.0-1b-speech`                  | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `granite-speech-4.1-2b`                  | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `granite-speech-4.1-2b-plus`             | Handy VAD chunks (expected) | Transcript prefix only, with matching audio; not used across Handy chunks   |
| `medasr`                                 | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-streaming-tiny`               | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `moonshine-tiny`                         | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-tiny-ar`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-tiny-ja`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-tiny-ko`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-tiny-uk`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-tiny-vi`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-tiny-zh`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base`                         | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base-ar`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base-ja`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base-ko`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base-uk`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base-vi`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-base-zh`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `moonshine-streaming-small`              | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `moonshine-streaming-medium`             | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `moss-transcribe-diarize`                | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `nemotron-speech-streaming-en-0.6b`      | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `parakeet-tdt_ctc-110m`                  | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `multitalker-parakeet-streaming-0.6b-v1` | Native streaming            | Continuous stream path; model-dependent state, not independent batch chunks |
| `parakeet-ctc-0.6b`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-rnnt-0.6b`                     | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-ctc-1.1b`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-primeline`                     | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-tdt-1.1b`                      | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-rnnt-1.1b`                     | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `parakeet-tdt_ctc-1.1b`                  | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `Qwen3-ASR-1.7B`                         | Handy VAD chunks (expected) | Context prompt API; Handy does not pass prior chunk text                    |
| `SenseVoiceSmall`                        | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `Voxtral-Mini-3B-2507`                   | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `Voxtral-Small-24B-2507`                 | Handy VAD chunks (expected) | No prior-transcript/context API advertised for this variant                 |
| `whisper-tiny`                           | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-tiny.en`                        | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-base`                           | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-base.en`                        | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-small.en`                       | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-small`                          | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-medium.en`                      | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-large-v3-turbo`                 | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `Breeze-ASR-25`                          | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-large`                          | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-large-v2`                       | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |
| `whisper-large-v3`                       | Native Whisper long-form    | Native previous-token history; context/prefix APIs                          |

## Acceptance before adopting the new strategy

- Use long utterances that pause mid-clause as well as at genuine sentence
  boundaries. Include a cut before “because”, a comma continuation and resumed
  dictation after a hesitation. Compare short whole-utterance references where
  the model's input limits permit it.
- Include proper names, acronyms, first-person “I”, German nouns and scripts
  without case. Do not implement blanket lowercasing or punctuation stripping
  at chunk starts: that can destroy correct text.
- Check raw, case-sensitive transcripts and boundary punctuation. WER after
  lowercasing/punctuation removal is insufficient for this acceptance question.
- Check duplicates, omitted words, token/timestamp seams, language and
  translation separately from presentation. Judge the raw ASR output before
  considering additional text processing.
- Only add context where the exact loaded model and task support the intended
  mechanism. Prefix use requires appropriate audio overlap and deduplication,
  not simply copying prior text into the next request.
- Record results for each decoder/model path actually tested. This checkpoint
  has source evidence only; no all-model quality or latency claim is made.

## Sources

- [Handy catalog snapshot](https://github.com/cjpais/Handy/blob/9e57a0d7929fcc6417ea0d6e54c1b57f62635edd/src-tauri/src/catalog/catalog.json).
- [Decode policy](https://github.com/cjpais/Handy/blob/9e57a0d7929fcc6417ea0d6e54c1b57f62635edd/src-tauri/src/managers/transcription/policy.rs),
  [chunk decoder](https://github.com/cjpais/Handy/blob/9e57a0d7929fcc6417ea0d6e54c1b57f62635edd/src-tauri/src/managers/transcription/chunked.rs),
  [cuts and text joining](https://github.com/cjpais/Handy/blob/9e57a0d7929fcc6417ea0d6e54c1b57f62635edd/src-tauri/src/audio_toolkit/chunking.rs).
- [Native prompting contract](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/prompting.md),
  [Whisper](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/whisper.md),
  [Parakeet Ultra](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/parakeet-ultra.md).
- Native source: `src/arch/parakeet/{model,decoder}.cpp`,
  `src/arch/whisper/{model,capabilities}.cpp`, `include/transcribe/whisper.h`,
  and per-family capability setup.
