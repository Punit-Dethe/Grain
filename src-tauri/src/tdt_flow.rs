//! [GRAIN] Capability-gated TDT Flow orchestration.
//!
//! This module owns Grain policy: bounded descriptor geometry, journal reads,
//! immediate retry, cancellation, and append-only delta assembly. The vendored
//! native layer owns only transactional decoder/projector/cursor state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use transcribe_cpp::{CancelToken, Feature, RunOptions, Session, TdtFlowOptions, TdtFlowWindow};

use crate::grain_audio_journal::{JournalAvailability, PcmJournal, PcmJournalReader};
use crate::managers::transcription::TranscriptionManager;
use crate::rolling::{Job, RollingMetrics};

const SAMPLE_RATE: u64 = 16_000;
const LEFT_CONTEXT_ENCODER_FRAMES: u64 = 70;
const RIGHT_CONTEXT_ENCODER_FRAMES: u64 = 13;
const MAX_DESCRIPTOR_ATTEMPTS: usize = 2;
// Keep every native recompute below 33 seconds. A rolling callback can cross
// the nominal 25-second hard cut, and queued commit hints can span still more;
// Planner therefore splits ownership ranges against this cap instead of
// assuming the cursor will always provide a small enough descriptor.
const REVIEWED_TDT_MAX_WINDOW_SAMPLES: usize = 33 * SAMPLE_RATE as usize;

pub(crate) enum TdtWorkerResult {
    Unsupported,
    Success {
        text: String,
        descriptors: usize,
        decode_rtf_ewma: Option<f64>,
    },
    Failure(String),
}

/// Grain-owned capability overlay. Keep the upstream catalog byte-identical;
/// only exact, reviewed catalog artifacts belong here. The filename prefix
/// covers every catalog quant while excluding custom/lookalike models.
const REVIEWED_TDT_FLOW_MODELS: &[&str] = &[
    "handy-computer/parakeet-tdt-0.6b-v2-gguf/parakeet-tdt-0.6b-v2-",
    "handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-",
];

fn is_reviewed_tdt_flow_model(model_id: &str) -> bool {
    model_id.ends_with(".gguf")
        && REVIEWED_TDT_FLOW_MODELS.iter().any(|prefix| {
            model_id
                .strip_prefix(prefix)
                .is_some_and(|quant| !quant.is_empty())
        })
}

fn should_route_tdt(model_id: &str, translate_to_english: bool) -> bool {
    is_reviewed_tdt_flow_model(model_id) && !translate_to_english
}

fn reviewed_tdt_flow_options() -> TdtFlowOptions {
    TdtFlowOptions {
        max_window_samples: REVIEWED_TDT_MAX_WINDOW_SAMPLES,
        // Catalog v2/v3 GGUFs predate Grain's capability metadata key.
        // Native begin still validates both Parakeet architecture and TDT
        // decoder head before any concrete cast or allocation.
        allow_unadvertised_tdt_head: true,
    }
}

fn append_delta(text: &mut String, delta: &str) {
    text.push_str(delta);
}

pub(crate) struct TdtRunConfig {
    pub(crate) model_id: String,
    pub(crate) language: Option<String>,
    pub(crate) translate_to_english: bool,
    pub(crate) conditioning: bool,
}

struct DescriptorDebtGuard<'a> {
    metrics: &'a RollingMetrics,
    fresh_frames: u64,
}

impl Drop for DescriptorDebtGuard<'_> {
    fn drop(&mut self) {
        self.metrics.end_decode(self.fresh_frames);
    }
}

struct Planner {
    stride: u64,
    left_context: u64,
    right_context: u64,
    max_window: u64,
    sequence: u64,
    committed: u64,
    finalized: bool,
}

impl Planner {
    fn new(stride: usize, max_window_samples: usize) -> Result<Self, String> {
        let stride = u64::try_from(stride).map_err(|_| "TDT stride exceeds u64".to_string())?;
        if stride == 0 {
            return Err("TDT stride is zero".into());
        }
        let left_context = stride
            .checked_mul(LEFT_CONTEXT_ENCODER_FRAMES)
            .ok_or_else(|| "TDT left context overflow".to_string())?;
        let right_context = stride
            .checked_mul(RIGHT_CONTEXT_ENCODER_FRAMES)
            .ok_or_else(|| "TDT right context overflow".to_string())?;
        let configured_max = u64::try_from(max_window_samples)
            .map_err(|_| "TDT max window exceeds u64".to_string())?;
        // Every non-final boundary is stride-aligned, so use the largest whole
        // encoder-frame window accepted by the configured native cap.
        let max_window = configured_max - configured_max % stride;
        let minimum_window = left_context
            .checked_add(right_context)
            .and_then(|value| value.checked_add(stride))
            .ok_or_else(|| "TDT minimum window overflow".to_string())?;
        if max_window < minimum_window {
            return Err(format!(
                "TDT max window {configured_max} is too small for left/right context and one owned encoder frame"
            ));
        }
        Ok(Self {
            stride,
            left_context,
            right_context,
            max_window,
            sequence: 0,
            committed: 0,
            finalized: false,
        })
    }

    fn non_final(&self, commit_hint: u64) -> Option<(TdtFlowWindow, u64)> {
        if self.finalized {
            return None;
        }
        let aligned_hint = commit_hint - commit_hint % self.stride;
        let context_start = self.committed.saturating_sub(self.left_context);
        let max_commit = context_start
            .checked_add(self.max_window)?
            .checked_sub(self.right_context)?;
        let commit = aligned_hint.min(max_commit);
        if commit <= self.committed {
            return None;
        }
        let context_end = commit.checked_add(self.right_context)?;
        Some((
            TdtFlowWindow {
                sequence: self.sequence,
                context_start_sample: context_start,
                fresh_start_sample: self.committed,
                commit_end_sample: commit,
                context_end_sample: context_end,
                final_window: false,
            },
            context_end,
        ))
    }

    /// Return the next bounded descriptor required after the journal closes.
    /// Large tails are emitted as one or more non-final windows whose right
    /// context is already present, followed by one exact (possibly unaligned)
    /// final window.
    fn closing_window(&self, total: u64) -> Option<TdtFlowWindow> {
        if self.finalized || total <= self.committed {
            return None;
        }
        let context_start = self.committed.saturating_sub(self.left_context);
        if total.saturating_sub(context_start) <= self.max_window {
            return Some(TdtFlowWindow {
                sequence: self.sequence,
                context_start_sample: context_start,
                fresh_start_sample: self.committed,
                commit_end_sample: total,
                context_end_sample: total,
                final_window: true,
            });
        }

        // The tail is too large for one final descriptor. Commit a bounded
        // prefix while retaining real recorded audio as right lookahead.
        let commit_hint = total.saturating_sub(self.right_context);
        self.non_final(commit_hint).map(|(window, _)| window)
    }

    fn commit(&mut self, window: &TdtFlowWindow) -> Result<(), String> {
        if window.sequence != self.sequence || window.fresh_start_sample != self.committed {
            return Err("TDT native update did not match the planned transaction".into());
        }
        self.committed = window.commit_end_sample;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| "TDT descriptor sequence overflow".to_string())?;
        self.finalized = window.final_window;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_worker(
    manager: &TranscriptionManager,
    journal: &Arc<PcmJournal>,
    reader: &mut PcmJournalReader,
    rx: &Receiver<Job>,
    cancelled: &Arc<AtomicBool>,
    cancel_token: &CancelToken,
    metrics: &RollingMetrics,
    config: TdtRunConfig,
    mut emit_preview: impl FnMut(&str),
) -> TdtWorkerResult {
    // Keep upstream's generated catalog untouched: this Grain overlay is the
    // human-reviewed routing tag. Untagged models take generic Flow without
    // checking out the engine twice; tagged artifacts fail loudly if their
    // native architecture/head is unexpectedly incompatible.
    if !should_route_tdt(&config.model_id, config.translate_to_english) {
        return TdtWorkerResult::Unsupported;
    }

    let result = manager.with_grain_tdt_flow_session(|session| {
        let advertised = session.model().supports(Feature::TdtFlow);
        log::info!(
            "[GRAIN] TDT Flow selected reviewed model '{}' (artifact capability metadata={})",
            config.model_id,
            advertised
        );

        session.set_cancel_token(cancel_token);
        let run_options = RunOptions {
            language: config.language.clone(),
            ..Default::default()
        };
        let flow_options = reviewed_tdt_flow_options();
        let outcome = run_capable_worker(
            session,
            journal,
            reader,
            rx,
            cancelled,
            metrics,
            config.conditioning,
            &run_options,
            &flow_options,
            &mut emit_preview,
        );
        session.clear_cancel_token();
        Ok(outcome)
    });

    match result {
        Ok(outcome) => outcome,
        Err(error) => TdtWorkerResult::Failure(format!(
            "TDT Flow could not acquire the loaded transcription session: {error}"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_capable_worker(
    session: &mut Session,
    journal: &PcmJournal,
    reader: &mut PcmJournalReader,
    rx: &Receiver<Job>,
    cancelled: &AtomicBool,
    metrics: &RollingMetrics,
    conditioning: bool,
    run_options: &RunOptions,
    flow_options: &TdtFlowOptions,
    emit_preview: &mut impl FnMut(&str),
) -> TdtWorkerResult {
    let mut flow = match session.tdt_flow(run_options, flow_options) {
        Ok(flow) => flow,
        Err(error) => return TdtWorkerResult::Failure(format!("TDT Flow begin failed: {error}")),
    };
    let stride = match flow.encoder_stride_samples() {
        Ok(stride) => stride,
        Err(error) => {
            flow.reset();
            return TdtWorkerResult::Failure(format!("TDT Flow geometry query failed: {error}"));
        }
    };
    let mut planner = match Planner::new(stride, flow_options.max_window_samples) {
        Ok(planner) => planner,
        Err(error) => {
            flow.reset();
            return TdtWorkerResult::Failure(error);
        }
    };
    let mut audio = Vec::<f32>::new();
    let mut text = String::new();
    let mut descriptors = 0usize;
    let mut decode_rtf_ewma = None;

    loop {
        if cancelled.load(Ordering::Acquire) {
            flow.reset();
            return TdtWorkerResult::Failure("rolling session cancelled".into());
        }
        let job = match rx.recv() {
            Ok(job) => job,
            Err(_) => {
                flow.reset();
                return TdtWorkerResult::Failure("rolling worker channel disconnected".into());
            }
        };
        match job {
            Job::Chunk(chunk) => {
                let fresh_frames = chunk.fresh_frames();
                let _queued_after = metrics.dequeue_descriptor(fresh_frames);
                metrics.begin_decode(chunk);
                let _debt_guard = DescriptorDebtGuard {
                    metrics,
                    fresh_frames,
                };
                let commit_hint = chunk.commit_end_frame;
                while let Some((window, target)) = planner.non_final(commit_hint) {
                    match journal.wait_for_frames(target, cancelled) {
                        Ok(JournalAvailability::Available) => {
                            if let Err(error) = process_descriptor(
                                &mut flow,
                                reader,
                                &mut audio,
                                &window,
                                conditioning,
                                cancelled,
                                &mut planner,
                                &mut text,
                                &mut descriptors,
                                &mut decode_rtf_ewma,
                            ) {
                                flow.reset();
                                return TdtWorkerResult::Failure(error);
                            }
                            emit_preview(&text);
                        }
                        // Finish owns terminal draining. Breaking here leaves
                        // this uncommitted range for bounded closing windows.
                        Ok(JournalAvailability::Closed) => break,
                        Ok(JournalAvailability::Cancelled) => {
                            flow.reset();
                            return TdtWorkerResult::Failure("rolling session cancelled".into());
                        }
                        Err(error) => {
                            flow.reset();
                            return TdtWorkerResult::Failure(format!(
                                "TDT Flow journal lookahead failed: {error}"
                            ));
                        }
                    }
                }
            }
            Job::Finish => {
                let total = journal.frame_count();
                if total == 0 && planner.committed == 0 {
                    flow.reset();
                    return TdtWorkerResult::Success {
                        text,
                        descriptors,
                        decode_rtf_ewma,
                    };
                }
                while !planner.finalized {
                    let Some(window) = planner.closing_window(total) else {
                        flow.reset();
                        return TdtWorkerResult::Failure(format!(
                            "TDT Flow final descriptor invariant failed: journal ended at {total}, committed through {}",
                            planner.committed
                        ));
                    };
                    if let Err(error) = process_descriptor(
                        &mut flow,
                        reader,
                        &mut audio,
                        &window,
                        conditioning,
                        cancelled,
                        &mut planner,
                        &mut text,
                        &mut descriptors,
                        &mut decode_rtf_ewma,
                    ) {
                        flow.reset();
                        return TdtWorkerResult::Failure(error);
                    }
                }
                if let Err(error) = flow.finish() {
                    flow.reset();
                    return TdtWorkerResult::Failure(format!("TDT Flow finish failed: {error}"));
                }
                return TdtWorkerResult::Success {
                    text,
                    descriptors,
                    decode_rtf_ewma,
                };
            }
        }
    }
}

enum DescriptorAttemptError {
    Cancelled,
    Failed(String),
}

fn retry_descriptor<T>(
    sequence: u64,
    mut operation: impl FnMut() -> Result<T, DescriptorAttemptError>,
) -> Result<(T, usize), String> {
    let mut last_error = None;
    for attempt in 1..=MAX_DESCRIPTOR_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok((value, attempt)),
            Err(DescriptorAttemptError::Cancelled) => {
                return Err("rolling session cancelled".into())
            }
            Err(DescriptorAttemptError::Failed(error)) => {
                last_error = Some(error);
                if attempt < MAX_DESCRIPTOR_ATTEMPTS {
                    log::warn!(
                        "[GRAIN] TDT descriptor {} failed; retrying the same transaction before advancing: {}",
                        sequence,
                        last_error.as_deref().unwrap_or("unknown error")
                    );
                }
            }
        }
    }
    Err(format!(
        "TDT Flow descriptor {} failed after {} same-descriptor attempts: {}",
        sequence,
        MAX_DESCRIPTOR_ATTEMPTS,
        last_error.unwrap_or_else(|| "unknown error".into())
    ))
}

#[allow(clippy::too_many_arguments)]
fn process_descriptor(
    flow: &mut transcribe_cpp::TdtFlow<'_>,
    reader: &mut PcmJournalReader,
    audio: &mut Vec<f32>,
    window: &TdtFlowWindow,
    conditioning: bool,
    cancelled: &AtomicBool,
    planner: &mut Planner,
    text: &mut String,
    descriptors: &mut usize,
    decode_rtf_ewma: &mut Option<f64>,
) -> Result<(), String> {
    reader
        .read_f32_range(
            window.context_start_sample,
            window.context_end_sample,
            audio,
        )
        .map_err(|error| format!("TDT Flow journal read failed: {error}"))?;
    if conditioning {
        crate::audio_toolkit::audio::normalize_gain(audio);
    }

    let started = Instant::now();
    let (update, attempt) = retry_descriptor(window.sequence, || {
        if cancelled.load(Ordering::Acquire) {
            return Err(DescriptorAttemptError::Cancelled);
        }
        flow.process(audio, window).map_err(|error| {
            if cancelled.load(Ordering::Acquire) || flow.was_aborted() {
                DescriptorAttemptError::Cancelled
            } else {
                DescriptorAttemptError::Failed(format!(
                    "{}; descriptor geometry sequence={} context=[{}..{}] fresh=[{}..{}] samples={} final={}",
                    error,
                    window.sequence,
                    window.context_start_sample,
                    window.context_end_sample,
                    window.fresh_start_sample,
                    window.commit_end_sample,
                    audio.len(),
                    window.final_window,
                ))
            }
        })
    })?;
    if update.sequence != window.sequence
        || update.committed_end_sample != window.commit_end_sample
        || update.final_window != window.final_window
    {
        return Err("TDT Flow returned a mismatched transactional update".into());
    }
    planner.commit(window)?;
    append_delta(text, &update.text_delta);
    *descriptors += 1;
    let elapsed = started.elapsed();
    let owned_seconds = window
        .commit_end_sample
        .saturating_sub(window.fresh_start_sample) as f64
        / SAMPLE_RATE as f64;
    if owned_seconds > 0.0 {
        let sample = elapsed.as_secs_f64() / owned_seconds;
        *decode_rtf_ewma = Some(
            decode_rtf_ewma
                .map(|previous| previous * 0.8 + sample * 0.2)
                .unwrap_or(sample),
        );
    }
    log::info!(
        "[GRAIN] TDT descriptor {} committed [{:.2}..{:.2}]s with context [{:.2}..{:.2}]s in {:.2}s (attempt {})",
        window.sequence,
        window.fresh_start_sample as f64 / SAMPLE_RATE as f64,
        window.commit_end_sample as f64 / SAMPLE_RATE as f64,
        window.context_start_sample as f64 / SAMPLE_RATE as f64,
        window.context_end_sample as f64 / SAMPLE_RATE as f64,
        elapsed.as_secs_f64(),
        attempt,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_requires_an_exact_reviewed_catalog_model() {
        for model in [
            "handy-computer/parakeet-tdt-0.6b-v2-gguf/parakeet-tdt-0.6b-v2-Q8_0.gguf",
            "handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-Q4_K_M.gguf",
        ] {
            assert!(is_reviewed_tdt_flow_model(model));
            assert!(should_route_tdt(model, false));
            assert!(!should_route_tdt(model, true));
        }

        for model in [
            "handy-computer/parakeet-tdt-0.6b-v1-gguf/parakeet-tdt-0.6b-v1-Q8_0.gguf",
            "handy-computer/parakeet-tdt-0.6b-v2-gguf",
            "local/parakeet-tdt-0.6b-v2-Q8_0.gguf",
            "handy-computer/parakeet-tdt-0.6b-v2-gguf/parakeet-tdt-0.6b-v2-Q8_0.bin",
        ] {
            assert!(
                !is_reviewed_tdt_flow_model(model),
                "unexpected tag match: {model}"
            );
            assert!(!should_route_tdt(model, false));
        }
    }

    #[test]
    fn delta_assembly_is_exact_append_only_concatenation() {
        let mut text = String::new();
        for delta in ["Hello", ", ", "WORLD", "!", " Next?"] {
            append_delta(&mut text, delta);
        }
        assert_eq!(text, "Hello, WORLD! Next?");
    }

    #[test]
    fn failed_descriptor_retries_before_next_sequence() {
        let mut calls = Vec::new();
        let mut first_attempt = true;
        let (value, attempts) = retry_descriptor(7, || {
            calls.push(7);
            if std::mem::take(&mut first_attempt) {
                Err(DescriptorAttemptError::Failed("transient".into()))
            } else {
                Ok("delta")
            }
        })
        .unwrap();
        assert_eq!(value, "delta");
        assert_eq!(attempts, 2);
        assert_eq!(calls, vec![7, 7]);

        retry_descriptor(8, || {
            calls.push(8);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, vec![7, 7, 8]);
    }

    #[test]
    fn cancelled_descriptor_is_not_retried() {
        let mut calls = 0;
        let error = retry_descriptor::<()>(3, || {
            calls += 1;
            Err(DescriptorAttemptError::Cancelled)
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(error.contains("cancelled"));
    }

    #[test]
    fn planner_has_strict_left_owned_right_ranges() {
        let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        let (first, target) = planner.non_final(16_000 * 15).unwrap();
        assert_eq!(first.sequence, 0);
        assert_eq!(first.context_start_sample, 0);
        assert_eq!(first.fresh_start_sample, 0);
        assert!(first.fresh_start_sample < first.commit_end_sample);
        assert_eq!(first.context_end_sample, target);
        assert_eq!(
            first.context_end_sample - first.commit_end_sample,
            13 * 1_280
        );
        planner.commit(&first).unwrap();

        let (second, _) = planner.non_final(16_000 * 30).unwrap();
        assert_eq!(second.fresh_start_sample, first.commit_end_sample);
        assert_eq!(
            second.context_start_sample,
            second.fresh_start_sample.saturating_sub(70 * 1_280)
        );
    }

    #[test]
    fn reviewed_cap_accepts_descriptor_two_after_a_full_rolling_hard_cut() {
        let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        // Cursor reconstructed from the report: descriptor 1 committed through
        // 42.56 s, then continuous speech reached the unchanged 25 s hard cut.
        planner.sequence = 2;
        planner.committed = 680_960;
        let (window, _) = planner
            .non_final(planner.committed + 25 * SAMPLE_RATE)
            .unwrap();
        let window_samples = window.context_end_sample - window.context_start_sample;

        assert_eq!(window.context_start_sample, 591_360); // 36.96 s
        assert_eq!(window.commit_end_sample, 1_080_320); // 67.52 s
        assert_eq!(window.context_end_sample, 1_096_960); // 68.56 s
        assert_eq!(window_samples, 505_600); // 31.60 s
        assert!(window_samples > TdtFlowOptions::default().max_window_samples as u64);
        assert!(window_samples <= reviewed_tdt_flow_options().max_window_samples as u64);
    }

    #[test]
    fn planner_splits_the_oversized_descriptors_from_the_runtime_report() {
        for (sequence, committed, commit_hint) in [(1, 330_240, 752_640), (6, 1_770_240, 2_215_680)]
        {
            let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
            planner.sequence = sequence;
            planner.committed = committed;
            let mut windows = Vec::new();

            while planner.committed < commit_hint {
                let (window, _) = planner.non_final(commit_hint).unwrap();
                assert_eq!(window.fresh_start_sample, planner.committed);
                assert!(
                    window.context_end_sample - window.context_start_sample
                        <= REVIEWED_TDT_MAX_WINDOW_SAMPLES as u64
                );
                planner.commit(&window).unwrap();
                windows.push(window);
            }

            assert_eq!(planner.committed, commit_hint);
            assert!(windows.len() >= 2, "reported range must be split");
        }
    }

    #[test]
    fn closing_a_large_unaligned_tail_stays_bounded_and_finishes_exactly() {
        let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        let total = 3 * REVIEWED_TDT_MAX_WINDOW_SAMPLES as u64 + 777;
        let mut windows = Vec::new();

        while !planner.finalized {
            let window = planner.closing_window(total).unwrap();
            assert_eq!(window.fresh_start_sample, planner.committed);
            assert!(
                window.context_end_sample - window.context_start_sample
                    <= REVIEWED_TDT_MAX_WINDOW_SAMPLES as u64
            );
            planner.commit(&window).unwrap();
            windows.push(window);
        }

        assert!(windows.len() > 1);
        assert!(windows[..windows.len() - 1]
            .iter()
            .all(|window| !window.final_window));
        let final_window = windows.last().unwrap();
        assert!(final_window.final_window);
        assert_eq!(final_window.commit_end_sample, total);
        assert_eq!(planner.committed, total);
    }

    #[test]
    fn planner_rejects_a_cap_that_cannot_hold_context_and_owned_audio() {
        let too_small =
            ((LEFT_CONTEXT_ENCODER_FRAMES + RIGHT_CONTEXT_ENCODER_FRAMES) * 1_280) as usize;
        assert!(Planner::new(1_280, too_small).is_err());
    }

    #[test]
    fn planner_state_is_bounded_over_long_session() {
        let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        for index in 1..=100_000u64 {
            let hint = index * 16 * 1_280;
            let (window, _) = planner.non_final(hint).unwrap();
            assert_eq!(window.fresh_start_sample, planner.committed);
            planner.commit(&window).unwrap();
        }
        assert_eq!(planner.sequence, 100_000);
        assert_eq!(
            std::mem::size_of::<Planner>(),
            7 * std::mem::size_of::<u64>()
        );
    }

    #[test]
    fn terminal_close_always_leaves_a_final_owned_range() {
        let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        let (candidate, required_total) = planner.non_final(16_000).unwrap();

        // If the journal closes before right lookahead arrives, the candidate
        // was never committed and its owned range becomes the final window.
        let closed_early = planner.closing_window(candidate.commit_end_sample).unwrap();
        assert_eq!(closed_early.fresh_start_sample, 0);
        assert_eq!(closed_early.commit_end_sample, candidate.commit_end_sample);

        // If the non-final descriptor succeeds, wait_for_frames required the
        // positive right context first. Closing afterward therefore still has
        // an owned tail to submit as final_window=true.
        assert!(required_total > candidate.commit_end_sample);
        planner.commit(&candidate).unwrap();
        let closed_after_commit = planner.closing_window(required_total).unwrap();
        assert_eq!(
            closed_after_commit.fresh_start_sample,
            candidate.commit_end_sample
        );
        assert_eq!(closed_after_commit.commit_end_sample, required_total);
        assert!(closed_after_commit.final_window);
    }

    #[test]
    fn planner_final_consumes_exact_unaligned_tail_once() {
        let mut planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        let (first, _) = planner.non_final(16_000).unwrap();
        planner.commit(&first).unwrap();
        let final_window = planner.closing_window(16_777).unwrap();
        assert!(final_window.final_window);
        assert_eq!(final_window.fresh_start_sample, first.commit_end_sample);
        assert_eq!(final_window.commit_end_sample, 16_777);
        assert_eq!(final_window.context_end_sample, 16_777);
        planner.commit(&final_window).unwrap();
        assert!(planner.closing_window(16_777).is_none());
    }

    #[test]
    fn planner_never_emits_zero_owned_descriptor() {
        let planner = Planner::new(1_280, REVIEWED_TDT_MAX_WINDOW_SAMPLES).unwrap();
        assert!(planner.non_final(1_279).is_none());
        assert!(planner.closing_window(0).is_none());
    }
}
