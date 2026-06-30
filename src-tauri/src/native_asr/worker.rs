//! [GRAIN] M6: Native ASR session driver + event bridge.
//!
//! Blocking inference runs here, off the audio and async threads (the plan's
//! isolation rule). [`drive_session`] pulls frames from a source, pushes them
//! into a backend session, stabilizes the raw events, and emits pill-facing
//! [`DaemonEvent`]s through a sink. It is generic over the frame source and the
//! sink, so the scripted fake backend + a collecting sink exercise the whole
//! pipeline with no Tauri, audio, model, or network (Milestone 6 exit).
//!
//! The `drive_session`/manager glue is consumed by the Native ASR action in
//! Milestone 7; allow it ahead of that wiring.
#![allow(dead_code)]

use grain_asr_core::events::{AsrEvent, Stability};
use grain_asr_core::model::AsrModelSpec;
use grain_asr_core::session::{AsrSessionConfig, AudioFrame, NativeAsrBackend};
use grain_asr_core::stabilizer::{StabilizerConfig, TranscriptStabilizer};
use grain_core::DaemonEvent;

/// Map a stabilized [`AsrEvent`] to the pill-facing [`DaemonEvent`]. Per-word
/// timing is intentionally dropped from the event bus (the low-RAM pill never
/// renders it); the worker keeps words for the history/paste step.
pub fn to_daemon(ev: AsrEvent) -> DaemonEvent {
    match ev {
        AsrEvent::Partial {
            session_id,
            segment_id,
            text,
            stability,
            ..
        } => DaemonEvent::AsrPartial {
            session_id,
            segment_id,
            text,
            stable: stability == Stability::Stable,
        },
        AsrEvent::Commit {
            session_id,
            segment_id,
            text,
            ..
        } => DaemonEvent::AsrCommit {
            session_id,
            segment_id,
            text,
        },
        AsrEvent::SegmentFinal {
            session_id,
            segment_id,
            text,
            ..
        } => DaemonEvent::AsrSegmentFinal {
            session_id,
            segment_id,
            text,
        },
        AsrEvent::SessionFinal { session_id, text } => {
            DaemonEvent::AsrSessionFinal { session_id, text }
        }
        AsrEvent::Error {
            session_id,
            recoverable,
            message,
        } => DaemonEvent::AsrError {
            session_id,
            recoverable,
            message,
        },
    }
}

/// What the frame source hands the worker on each pull.
pub enum FrameCmd {
    /// One captured frame to push into the session.
    Frame(AudioFrame),
    /// Force a decode of buffered audio (e.g. a VAD endpoint) without ending.
    Flush,
    /// End the session: finish, emit the session final, and return.
    Stop,
}

/// Drive one Native ASR session to completion. Returns the assembled final text
/// (also emitted as [`DaemonEvent::AsrSessionFinal`]) for the caller's
/// history/paste step.
///
/// On a backend error the loop still finalizes from what was committed, so a
/// crash never loses the user's dictation (refinement: crash-safe finalization).
pub fn drive_session(
    mut backend: Box<dyn NativeAsrBackend>,
    spec: &AsrModelSpec,
    config: AsrSessionConfig,
    stab_config: StabilizerConfig,
    mut next: impl FnMut() -> FrameCmd,
    mut emit: impl FnMut(DaemonEvent),
) -> anyhow::Result<String> {
    let caps = backend.load(spec)?;
    let session_id = config.session_id;
    let mut session = backend.start_session(config)?;
    let mut stab = TranscriptStabilizer::new(session_id, caps, stab_config);

    loop {
        match next() {
            FrameCmd::Frame(f) => {
                for raw in session.push_audio(f)? {
                    for ev in stab.ingest(raw) {
                        emit(to_daemon(ev));
                    }
                }
            }
            FrameCmd::Flush => {
                for raw in session.flush()? {
                    for ev in stab.ingest(raw) {
                        emit(to_daemon(ev));
                    }
                }
            }
            FrameCmd::Stop => break,
        }
    }

    for raw in session.finish()? {
        for ev in stab.ingest(raw) {
            emit(to_daemon(ev));
        }
    }

    let sf = stab.session_final();
    let final_text = match &sf {
        AsrEvent::SessionFinal { text, .. } => text.clone(),
        _ => String::new(),
    };
    emit(to_daemon(sf));
    backend.unload();
    Ok(final_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_asr_core::events::AsrRawEvent;
    use grain_asr_core::model::{
        AsrBackendKind, AsrCapabilities, AsrModelFiles, AsrModelSpec, MemoryProfile,
    };
    use grain_asr_core::testing::ScriptedBackend;
    use std::sync::Arc;

    fn spec() -> AsrModelSpec {
        AsrModelSpec {
            id: "fake".into(),
            name: "Fake".into(),
            backend: AsrBackendKind::SherpaOnnx,
            files: AsrModelFiles::SherpaTransducer {
                encoder: "e".into(),
                decoder: "d".into(),
                joiner: "j".into(),
                tokens: "t".into(),
                config: None,
            },
            sample_rate_hz: 16_000,
            languages: vec!["en".into()],
            capabilities: AsrCapabilities::streaming_minimal(),
            memory: MemoryProfile { approx_mb: 64 },
        }
    }

    fn frame() -> AudioFrame {
        AudioFrame::new(Arc::from(vec![0.0f32; 160].into_boxed_slice()), 16_000)
    }

    #[test]
    fn scripted_backend_drives_daemon_events() {
        let caps = AsrCapabilities::streaming_minimal();
        let script = vec![
            vec![AsrRawEvent::Partial {
                segment_id: 0,
                revision: 0,
                text: "hello".into(),
                words: vec![],
            }],
            vec![AsrRawEvent::Partial {
                segment_id: 0,
                revision: 1,
                text: "hello world".into(),
                words: vec![],
            }],
            vec![AsrRawEvent::BackendFinal {
                segment_id: 0,
                text: "hello world".into(),
                words: vec![],
            }],
        ];
        let backend = Box::new(ScriptedBackend::new(caps, script));

        // Frame source: three frames, then stop.
        let mut pulled = 0;
        let next = move || {
            pulled += 1;
            if pulled <= 3 {
                FrameCmd::Frame(frame())
            } else {
                FrameCmd::Stop
            }
        };

        let mut events = Vec::new();
        let final_text = drive_session(
            backend,
            &spec(),
            AsrSessionConfig {
                session_id: 7,
                ..Default::default()
            },
            StabilizerConfig::default(),
            next,
            |e| events.push(e),
        )
        .unwrap();

        assert_eq!(final_text, "hello world");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DaemonEvent::AsrCommit { .. })),
            "expected at least one AsrCommit"
        );
        assert!(events.iter().any(|e| matches!(
            e,
            DaemonEvent::AsrSegmentFinal { text, .. } if text == "hello world"
        )));
        match events.last().unwrap() {
            DaemonEvent::AsrSessionFinal { session_id, text } => {
                assert_eq!(*session_id, 7);
                assert_eq!(text, "hello world");
            }
            other => panic!("expected AsrSessionFinal last, got {other:?}"),
        }
    }

    /// M8 stop-while-decoding: if Stop arrives before the backend has emitted its
    /// final, `finish()` drains the remaining decode so the transcript is not
    /// lost. Here only two frames are pushed (two scripted batches) before Stop;
    /// the third batch (the BackendFinal) is drained on finish.
    #[test]
    fn stop_mid_stream_drains_remaining_on_finish() {
        let caps = AsrCapabilities::streaming_minimal();
        let script = vec![
            vec![AsrRawEvent::Partial {
                segment_id: 0,
                revision: 0,
                text: "hello".into(),
                words: vec![],
            }],
            vec![AsrRawEvent::Partial {
                segment_id: 0,
                revision: 1,
                text: "hello world".into(),
                words: vec![],
            }],
            vec![AsrRawEvent::BackendFinal {
                segment_id: 0,
                text: "hello world".into(),
                words: vec![],
            }],
        ];
        let backend = Box::new(ScriptedBackend::new(caps, script));

        // Stop after only two frames — the BackendFinal batch is still queued.
        let mut pulled = 0;
        let next = move || {
            pulled += 1;
            if pulled <= 2 {
                FrameCmd::Frame(frame())
            } else {
                FrameCmd::Stop
            }
        };

        let mut events = Vec::new();
        let final_text = drive_session(
            backend,
            &spec(),
            AsrSessionConfig::default(),
            StabilizerConfig::default(),
            next,
            |e| events.push(e),
        )
        .unwrap();

        assert_eq!(final_text, "hello world", "remaining decode must not be lost");
        assert!(events.iter().any(|e| matches!(
            e,
            DaemonEvent::AsrSegmentFinal { text, .. } if text == "hello world"
        )));
    }
}
