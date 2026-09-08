//! The typed event stream the headless core broadcasts to every subscriber
//! (the pill, the settings window, the local server's `/events`).
//!
//! Replaces Handy's ~15 raw untyped `app.emit("...")` strings with one typed
//! enum carried over a `tokio::sync::broadcast` channel — multiple subscribers,
//! and no Tauri dependency.

use serde::{Deserialize, Serialize};

/// Serde variant names accepted by `onEvent:<Variant>` activations.
///
/// This vocabulary lives beside [`DaemonEvent`] so author tooling and registry
/// CI do not maintain their own copies of the public event contract.
// NOTE: hand-maintained, and — unlike `variant_name` below — the compiler will
// NOT tell you when it is out of date. A variant missing from this list gets
// `daemon_event_capability(..) == None`, which `events_auth::required_capability`
// turns into a panic on the first broadcast. Add the name here in the same edit
// that adds the variant.
pub const DAEMON_EVENT_VARIANTS: &[&str] = &[
    "RecordingStarted",
    "RecordingStopped",
    "SessionCancelled",
    "PromptRecordingChanged",
    "ChunkComplete",
    "TranscriptionComplete",
    "ProcessingComplete",
    "ModelLoading",
    "ModelLoaded",
    "ModelUnloaded",
    "ModelError",
    "ModelDownloadProgress",
    "AudioLevel",
    "PromptChanged",
    "PromptActive",
    "ActionChoice",
    "ActionChoiceClosed",
    "ActionResult",
    "AgentFollowupOffer",
    "AgentFollowupClear",
    "AgentInputShow",
    "AgentInputHide",
    "AgentInputSaved",
    "AgentInputSubmitRequest",
    "ExtensionRecommend",
    "ExtensionRecommendClear",
    "ShowOverlay",
    "HideOverlay",
    "PasteError",
    "PasteMissed",
    "PasteMissedClear",
    "PasteCatchDisabled",
    "OverlayConfig",
    "ThemeConfig",
    "AsrStreamText",
    "AsrPartial",
    "AsrCommit",
    "AsrSegmentFinal",
    "AsrSessionFinal",
    "AsrError",
    "ExtensionDisabled",
    "PillSkin",
    "PillIcon",
];

/// Edge length of the icon the core hands the pill, in pixels. Fixed so the
/// wire payload is always exactly `PILL_ICON_PX² × 4` bytes and neither side has
/// to negotiate a size; the pill scales it down to whatever its skin draws at.
pub const PILL_ICON_PX: usize = 64;

/// Capability required to receive (or be woken by) a daemon event variant.
/// Unknown names return `None` so manifest tooling can reject them rather than
/// silently assigning a broad capability.
pub fn daemon_event_capability(variant: &str) -> Option<&'static str> {
    match variant {
        "ChunkComplete"
        | "TranscriptionComplete"
        | "ProcessingComplete"
        | "AsrStreamText"
        | "AsrPartial"
        | "AsrCommit"
        | "AsrSegmentFinal"
        | "AsrSessionFinal" => Some("events:transcripts"),
        "AudioLevel" => Some("events:audio-levels"),
        known if DAEMON_EVENT_VARIANTS.contains(&known) => Some("events:sessions"),
        _ => None,
    }
}

/// [GRAIN] Which brain the NATIVE agent input card serves — purely
/// presentational (the core routes submits by its own `AgentState.mode`). It
/// lets the ONE pill surface render the right variant without a second window:
/// `Assist` keeps the original card; the Grain Space kinds (`Capture`,
/// `Recall`) anchor to the TOP and relabel the card ("Noting…"/"Save Note" vs
/// "Listening…"/"Confirm"). No extra RAM — same window, same pixmap, just
/// different strings/anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, specta::Type)]
pub enum AgentInputKind {
    /// The generic assistant (operates on selection/field). Original card.
    #[default]
    Assist,
    /// Grain Space capture (note authoring) — "Noting…", title+body, "Save Note".
    Capture,
    /// Grain Space recall (memory question) — "Listening…", single ask field.
    Recall,
}

/// Where the single pill anchors on screen (`None` = never show). Lives in the
/// SDK because it crosses the wire inside [`DaemonEvent::OverlayConfig`]; it is
/// also the persisted `overlay_position` setting (grain-core re-exports it).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
    /// [GRAIN] Vertically centered — the Native ASR Studio Window's natural home
    /// (a tall content box reads poorly hugging an edge); also selectable for
    /// the small pill.
    Center,
}

/// [GRAIN] What to actually paint — the user's `theme` preference already
/// resolved against the OS. Lives in the SDK because it crosses the wire inside
/// [`DaemonEvent::ThemeConfig`]; grain-core re-exports it next to the
/// `ThemeMode` preference that produces it.
///
/// Only the resolved answer travels. A surface that received `System` would
/// have to ask the OS itself, and the pill, the capsule and a sandboxed
/// extension frame are three different toolkits with three different ways of
/// getting that wrong.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedTheme {
    Light,
    Dark,
}

/// What a recording session is for. Drives the "what you end with wins" logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum SessionMode {
    /// Plain dictation — stop pastes the raw transcript.
    Dictation,
    /// Voice-to-AI — stop sends the transcript to the LLM with the active prompt.
    VoiceToAI,
    /// Batch — record fully, then transcribe once (no rolling window).
    Batch,
    /// [GRAIN] Native ASR — live streaming dictation. Tells the pill to switch
    /// from the small collapsed capsule to the expanded Studio Window, since
    /// only this mode has a stabilized live-text stream (`Asr*` events) worth
    /// displaying.
    NativeAsr,
}

/// [GRAIN] One row on the Extension Mode selection surface
/// (`docs/Extensions V1/PLAN.md` §3, §6b): a candidate extension the user may
/// hand the request to. Carried in [`DaemonEvent::ExtensionRecommend`] so the
/// pill can draw the ranked list without reaching back into the host.
///
/// The list the pill receives is the WHOLE installed searchable pool, ordered by
/// confidence — the recommended ones first (a real `signal`), then the rest so
/// the user can scroll to any installed extension and pick it by hand. `signal`
/// is `"named"`, `"topical"`, or `"none"` (an unranked pool member shown only so
/// it is reachable); `score` is the recommendation score, `0.0` for `"none"`.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct RecommendCandidate {
    pub extension_id: String,
    /// The extension's display name (already resolved from its manifest).
    pub name: String,
    /// The one-line purpose from its `recommend` block, or empty.
    pub purpose: String,
    /// `"named"` | `"topical"` | `"none"` — how (or whether) it was ranked.
    pub signal: String,
    /// The recommendation score, or `0.0` for an unranked (`"none"`) row.
    pub score: f32,
    /// Pre-downscaled icon (premultiplied RGBA, `PILL_ICON_PX`², base64), or
    /// `None` — the pill draws an initial badge instead. The icon runtime that
    /// populates this (embed the 512² master in the built pack, write it at
    /// install, host-downscale) lands after the surface; the field flows through
    /// now so no wire change is needed when it does (§13.3 deferral).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// One event broadcast by the daemon. `Clone` so every subscriber gets a copy;
/// `Serialize`/`Deserialize` so it can cross the local WebSocket to the pill.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub enum DaemonEvent {
    // -- Recording lifecycle --
    RecordingStarted {
        session_id: u64,
        mode: SessionMode,
        /// Present only when a third-party extension owns the slow stage. The
        /// pill renders this host-derived display name; extension code controls
        /// neither the indicator nor its layout.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
    },
    RecordingStopped {
        session_id: u64,
    },
    SessionCancelled {
        session_id: u64,
    },

    /// [GRAIN] Prompt Record: the user clicked the compact pill mid-recording to
    /// begin dictating an AI *instruction* (everything spoken after this point is
    /// a prompt for post-processing, not content). Drives the pill's blue dot
    /// tint. One-way for now — `active` goes true and stays true until the session
    /// ends (no toggle-off, by design, to keep the interaction dead simple).
    PromptRecordingChanged {
        session_id: u64,
        active: bool,
    },

    // -- Rolling-window progress --
    /// Intermediate assembled text after a chunk finalized.
    ChunkComplete {
        session_id: u64,
        chunk_idx: u32,
        text: String,
    },
    /// Final assembled transcript for the session.
    TranscriptionComplete {
        session_id: u64,
        text: String,
    },
    /// LLM post-processing finished.
    ProcessingComplete {
        session_id: u64,
        text: String,
    },

    // -- Model lifecycle (replaces `model-state-changed`) --
    ModelLoading {
        model_id: String,
    },
    ModelLoaded {
        model_id: String,
    },
    ModelUnloaded,
    ModelError {
        error: String,
    },
    /// Download/verify/extract progress (replaces `model-download-progress` etc.).
    ModelDownloadProgress {
        model_id: String,
        progress: f32,
    },

    // -- Pill UI feed --
    /// Per-bucket audio energy driving the Aura Core dots (replaces `mic-level`).
    AudioLevel {
        levels: Vec<f32>,
    },
    /// Active prompt changed mid-speech → pill riser (← name →).
    PromptChanged {
        name: String,
    },
    /// [GRAIN] The active prompt's name, announced WITHOUT a switch — emitted at
    /// session start so the pill's switcher can render the current title the
    /// moment it is revealed by hover. Deliberately distinct from
    /// [`DaemonEvent::PromptChanged`]: this one must never arm the riser or
    /// reveal an idle pill, it only refreshes the label.
    PromptActive {
        name: String,
    },
    /// [GRAIN] A routed action needs the user to decide
    /// (`docs/Action Routing/PLAN.md` §7). The pill reveals a capsule listing
    /// the numbered options; digits pick, Escape cancels, and it releases on
    /// its own after a moment.
    ///
    /// `heard` is what the recogniser produced, shown verbatim. It is the whole
    /// reason a chooser is not a dead end: when the wrong options appear, the
    /// answer is almost always visible in that line.
    ActionChoice {
        /// `action`, `provider`, `entity`, or `confirm` — the four things this
        /// one capsule is used for.
        kind: String,
        heard: String,
        /// One line per option, already ordered. Never more than a handful:
        /// past that there is no honest question to ask.
        options: Vec<String>,
    },
    /// [GRAIN] The choice was answered, cancelled, or timed out — hide the
    /// capsule. Emitted exactly once per [`DaemonEvent::ActionChoice`].
    ActionChoiceClosed,
    /// [GRAIN] What became of a routed action, for the pill's brief
    /// confirmation. `ok` false means it did not run.
    ActionResult {
        ok: bool,
        message: String,
    },
    /// [GRAIN] Quick Agent: a reply was just auto-pasted at the cursor. The pill
    /// briefly reveals with an "ASK FOLLOW-UP · <shortcut>" affordance; clicking
    /// it (or pressing the shortcut) reopens the Agent expanded with the
    /// conversation restored. `shortcut` is the human-readable binding label.
    AgentFollowupOffer {
        shortcut: String,
    },
    /// [GRAIN] Withdraw the follow-up offer (panel opened, offer expired, or a
    /// new session started).
    AgentFollowupClear,

    /// [GRAIN] Show the NATIVE agent input (the summon surface): a bottom-center
    /// card that records by default and expands into a typing field the moment
    /// the user types. `selection_chars` feeds the selection chip;
    /// `type_to_expand` mirrors the setting (when false, typing while listening
    /// is ignored until the user expands explicitly).
    AgentInputShow {
        selection_chars: u32,
        #[serde(default)]
        type_to_expand: bool,
        /// Which brain this summon serves — drives the card variant (anchor +
        /// labels). Defaults to `Assist` for back-compat with older cores.
        #[serde(default)]
        kind: AgentInputKind,
    },
    /// [GRAIN] Hide the native agent input (submitted / cancelled / superseded).
    AgentInputHide,
    /// [GRAIN] Grain Space capture succeeded HEADLESSLY: tell the card to play a
    /// brief in-place "Saved" confirmation (green dot sweep + "Saved") before the
    /// core hides it. No new pill/surface — the same summon card confirms itself.
    AgentInputSaved,
    /// [GRAIN] The core's transient global Enter fired while the agent input is
    /// up. The pill owns the typed text, so it answers with
    /// `AgentInputSubmitText` (typing) or `AgentInputSubmitVoice` (recording).
    AgentInputSubmitRequest,

    /// [GRAIN] Extension Mode (`docs/Extensions V1/PLAN.md` §3, §6b): a captured
    /// request has been ranked — bring up the native selection surface. The pill
    /// presents the ranked list (`candidates`, whole searchable pool, best
    /// first), a type bar to search it, and a clean border on the top pick; a
    /// click hands that extension the request (`PillAction::ExtensionChoose`),
    /// Esc cancels (`PillAction::ExtensionCancel`). `name_only` is true when the
    /// embedding model is absent (ranking was lexical-only, §5) so the surface
    /// can say so and offer the download.
    ExtensionRecommend {
        /// Host-generated nonce binding every reverse action to this exact
        /// presentation. The pill treats it as opaque.
        presentation_id: u64,
        candidates: Vec<RecommendCandidate>,
        #[serde(default)]
        name_only: bool,
    },
    /// [GRAIN] Withdraw the Extension Mode selection surface: the user chose or
    /// cancelled, Grain auto-sent (§5), or a new session superseded it. Idempotent:
    /// lifecycle races may clear an already-withdrawn surface and the pill treats
    /// that as a no-op.
    ExtensionRecommendClear,

    // -- Misc UI signals --
    ShowOverlay,
    HideOverlay,
    PasteError {
        error: String,
    },

    /// [GRAIN] Paste Catch: a transcript's paste provably missed the text field,
    /// so Grain is holding it on the clipboard instead of letting the restore
    /// destroy it. The pill shows an offer for the hold's lifetime; pressing
    /// `shortcut` (or plain Ctrl+V — the transcript really is on the clipboard)
    /// delivers it. `chars` is the transcript length, for a "42 words held"
    /// style hint without shipping the text itself to the surface.
    PasteMissed {
        shortcut: String,
        chars: u32,
    },
    /// [GRAIN] Withdraw the Paste Catch offer: delivered, superseded by a new
    /// dictation, or the hold expired and the clipboard was handed back.
    PasteMissedClear,
    /// [GRAIN] Paste Catch was turned off in settings. Unlike the ordinary hold
    /// clear above, this also withdraws the pill's independent three-second
    /// clipboard confirmation immediately.
    PasteCatchDisabled,

    /// Where the single pill should anchor — and whether to show at all
    /// (`OverlayPosition::None` = never show). Emitted on session start and when
    /// the user changes the position setting, so the pill can place/hide itself.
    OverlayConfig {
        position: OverlayPosition,
    },

    /// [GRAIN] Which colour scheme every surface should paint. Emitted when the
    /// user changes the preference and when the OS flips under `System`, so the
    /// pill and extension surfaces restyle without polling.
    ThemeConfig {
        theme: ResolvedTheme,
    },

    // -- Native ASR (real-time streaming dictation) --
    /// [GRAIN] transcribe-cpp streaming: the cumulative committed transcript so
    /// far (flicker-free, growing) plus the volatile `tentative` tail the model
    /// may still rewrite. The pill renders BOTH (Handy parity: its overlay shows
    /// committed + tentative) — the engine's auto-commit can stall for long
    /// stretches (often right after a sentence boundary), and without the tail
    /// the live preview visibly freezes even though decoding continues.
    AsrStreamText {
        session_id: u64,
        committed: String,
        #[serde(default)]
        tentative: String,
    },

    // The stabilized stream from the (legacy sherpa) Native ASR path. `AsrCommit`
    // text is immutable (safe to keep); `AsrPartial` text is volatile.
    /// Volatile tail for a segment. `stable` = the stabilizer is confident it
    /// won't change (held only by commit lag), else it may still be rewritten.
    AsrPartial {
        session_id: u64,
        segment_id: u64,
        text: String,
        stable: bool,
    },
    /// Newly committed (immutable) words appended to a segment's prefix.
    AsrCommit {
        session_id: u64,
        segment_id: u64,
        text: String,
    },
    /// A segment closed; `text` is its full final transcript.
    AsrSegmentFinal {
        session_id: u64,
        segment_id: u64,
        text: String,
    },
    /// The whole session closed; `text` is every segment joined (paste/history).
    AsrSessionFinal {
        session_id: u64,
        text: String,
    },
    /// A surfaced Native ASR error.
    AsrError {
        session_id: u64,
        recoverable: bool,
        message: String,
    },

    /// [GRAIN] Extension platform (SPEC §3.3): a scripted extension was
    /// auto-disabled after repeatedly failing its transform deadline (3 strikes).
    /// The Overview UI shows the reason; the user re-enables explicitly.
    ExtensionDisabled {
        id: String,
        reason: String,
    },

    /// [GRAIN] Which built-in look the collapsed pill should wear. Sent when the
    /// pill authenticates and whenever the user changes the `pill_skin` setting.
    /// Changing it resizes the pill window, so it is never sent per frame.
    PillSkin {
        #[serde(default)]
        skin: crate::PillSkin,
    },

    /// [GRAIN] The icon of whatever the user is dictating into, so the pill can
    /// show that it understands the surface rather than merely that it is on.
    ///
    /// `PILL_ICON_PX`² **premultiplied** RGBA, base64 — finished pixels, because
    /// resolving them means COM and the shell on Windows and entirely different
    /// machinery elsewhere, and none of that belongs in the pill process. `None`
    /// means "no icon, draw the plain state dot", which is also what the pill
    /// shows until a cold resolve lands (it never blocks a recording).
    ///
    /// Sent at most twice per session, never per frame.
    PillIcon {
        #[serde(default)]
        rgba: Option<String>,
    },
}

impl DaemonEvent {
    /// The serde (externally-tagged) variant name, **without serializing**.
    ///
    /// The extension host matches `onEvent:<Variant>` activations against this
    /// on every broadcast — including `AudioLevel`, which fires many times a
    /// second while recording — so it must not allocate. Serializing the event
    /// just to read its tag was measurably the wrong thing.
    ///
    /// The match is exhaustive on purpose: adding a variant without naming it
    /// here is a compile error, never a silently unmatchable activation.
    pub fn variant_name(&self) -> &'static str {
        use DaemonEvent::*;
        match self {
            RecordingStarted { .. } => "RecordingStarted",
            RecordingStopped { .. } => "RecordingStopped",
            SessionCancelled { .. } => "SessionCancelled",
            PromptRecordingChanged { .. } => "PromptRecordingChanged",
            ChunkComplete { .. } => "ChunkComplete",
            TranscriptionComplete { .. } => "TranscriptionComplete",
            ProcessingComplete { .. } => "ProcessingComplete",
            ModelLoading { .. } => "ModelLoading",
            ModelLoaded { .. } => "ModelLoaded",
            ModelUnloaded => "ModelUnloaded",
            ModelError { .. } => "ModelError",
            ModelDownloadProgress { .. } => "ModelDownloadProgress",
            ThemeConfig { .. } => "ThemeConfig",
            AudioLevel { .. } => "AudioLevel",
            PromptChanged { .. } => "PromptChanged",
            PromptActive { .. } => "PromptActive",
            ActionChoice { .. } => "ActionChoice",
            ActionChoiceClosed => "ActionChoiceClosed",
            ActionResult { .. } => "ActionResult",
            AgentFollowupOffer { .. } => "AgentFollowupOffer",
            AgentFollowupClear => "AgentFollowupClear",
            AgentInputShow { .. } => "AgentInputShow",
            AgentInputHide => "AgentInputHide",
            AgentInputSaved => "AgentInputSaved",
            AgentInputSubmitRequest => "AgentInputSubmitRequest",
            ExtensionRecommend { .. } => "ExtensionRecommend",
            ExtensionRecommendClear => "ExtensionRecommendClear",
            ShowOverlay => "ShowOverlay",
            HideOverlay => "HideOverlay",
            PasteError { .. } => "PasteError",
            PasteMissed { .. } => "PasteMissed",
            PasteMissedClear => "PasteMissedClear",
            PasteCatchDisabled => "PasteCatchDisabled",
            OverlayConfig { .. } => "OverlayConfig",
            AsrStreamText { .. } => "AsrStreamText",
            AsrPartial { .. } => "AsrPartial",
            AsrCommit { .. } => "AsrCommit",
            AsrSegmentFinal { .. } => "AsrSegmentFinal",
            AsrSessionFinal { .. } => "AsrSessionFinal",
            AsrError { .. } => "AsrError",
            ExtensionDisabled { .. } => "ExtensionDisabled",
            PillSkin { .. } => "PillSkin",
            PillIcon { .. } => "PillIcon",
        }
    }
}

#[cfg(test)]
mod variant_name_tests {
    use super::*;

    /// The name must equal serde's own tag, or activations silently never fire.
    #[test]
    fn variant_name_matches_the_serde_tag() {
        let cases = vec![
            DaemonEvent::RecordingStarted {
                session_id: 1,
                mode: SessionMode::Dictation,
                owner: None,
            },
            DaemonEvent::TranscriptionComplete {
                session_id: 1,
                text: "x".into(),
            },
            DaemonEvent::ModelUnloaded,
            DaemonEvent::AudioLevel { levels: vec![] },
            DaemonEvent::ExtensionRecommend {
                presentation_id: 7,
                candidates: vec![],
                name_only: true,
            },
            DaemonEvent::ThemeConfig {
                theme: ResolvedTheme::Dark,
            },
        ];
        for ev in cases {
            let tag = match serde_json::to_value(&ev).unwrap() {
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(m) => m.keys().next().unwrap().clone(),
                other => panic!("unexpected shape: {other:?}"),
            };
            assert_eq!(ev.variant_name(), tag);
            assert!(daemon_event_capability(&tag).is_some());
        }
    }

    #[test]
    fn event_capability_mapping_distinguishes_payload_classes() {
        assert_eq!(
            daemon_event_capability("TranscriptionComplete"),
            Some("events:transcripts")
        );
        assert_eq!(
            daemon_event_capability("RecordingStarted"),
            Some("events:sessions")
        );
        assert_eq!(
            daemon_event_capability("AudioLevel"),
            Some("events:audio-levels")
        );
        assert_eq!(
            daemon_event_capability("ExtensionRecommend"),
            Some("events:sessions")
        );
        assert_eq!(daemon_event_capability("NotARealEvent"), None);
    }

    #[test]
    fn extension_chooser_actions_keep_the_reverse_wire_shape() {
        let choose = serde_json::to_value(PillAction::ExtensionChoose {
            presentation_id: 7,
            extension_id: "com.example.translate".into(),
        })
        .unwrap();
        assert_eq!(choose["action"], "extension_choose");
        assert_eq!(choose["presentation_id"], 7);
        assert_eq!(choose["extension_id"], "com.example.translate");
        assert_eq!(
            serde_json::to_value(PillAction::ExtensionDownloadModel).unwrap()["action"],
            "extension_download_model"
        );
    }
}

/// [GRAIN] The reverse channel: actions the pill sends BACK to the core over the
/// same local WebSocket (the core's events server reads these). Kept tiny and
/// self-describing so the transport stays a single duplex JSON stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PillAction {
    /// [GRAIN] User clicked the compact pill mid-recording to enter Prompt Record
    /// mode. The core marks the current audio position as the content→instruction
    /// split point and echoes back `PromptRecordingChanged { active: true }`.
    PromptRecord,
    /// [GRAIN] User clicked the pill's Quick-Agent follow-up offer — reopen the
    /// Agent expanded with the retained conversation.
    AgentFollowup,
    /// [GRAIN] User clicked one of the switcher capsule's `‹`/`›` arrows.
    /// `delta` is the step through the prompt list (`-1` previous, `1` next) —
    /// the same cycle the switcher shortcut performs, so the core answers with
    /// `PromptChanged` exactly as it would for the keyboard.
    PromptCycle { delta: i32 },
    /// [GRAIN] User clicked the expanded (live transcription) card's cancel ×.
    /// Identical to pressing the Cancel shortcut: the core drops the recording,
    /// the transcript, and every session surface, then hides the pill.
    CancelSession,
    /// [GRAIN] Agent input: the user submitted TYPED text (expanded card).
    /// `title` is the optional Grain Space note title (Capture only; empty
    /// otherwise). `quick` = the user held Shift → Quick Agent (paste in place)
    /// instead of opening the panel (Assist only).
    AgentInputSubmitText {
        text: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        quick: bool,
    },
    /// [GRAIN] Agent input: submit the in-progress VOICE capture (compact card) —
    /// the core stops dictation, transcribes, and runs the instruction. `quick`
    /// as above (Shift held → Quick Agent).
    AgentInputSubmitVoice {
        #[serde(default)]
        quick: bool,
    },
    /// [GRAIN] Agent input: the user cancelled (Esc) — the core cancels dictation
    /// and destroys the pre-created panel.
    AgentInputCancel,
    /// [GRAIN] Agent input mode switch: `active: true` = the user started typing
    /// (core cancels the voice capture); `false` = the user tabbed back to voice
    /// (core restarts dictation).
    AgentInputTyping { active: bool },

    /// [GRAIN] Extension Mode: the user picked a row on the selection surface —
    /// hand this extension the whole captured request
    /// (`docs/Extensions V1/PLAN.md` §3). The core wakes it, delivers the
    /// request, and clears the surface.
    ExtensionChoose {
        presentation_id: u64,
        extension_id: String,
    },
    /// [GRAIN] Extension Mode: the user dismissed the selection surface (Esc /
    /// clicked away) without choosing. The core drops the pending request.
    ExtensionCancel { presentation_id: u64 },
    /// [GRAIN] Extension Mode is running in name-only mode and the user accepted
    /// the chooser's first-use offer to download the shared semantic model.
    ExtensionDownloadModel,
}
