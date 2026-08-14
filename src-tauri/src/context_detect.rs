//! [GRAIN] Context awareness — detect the foreground app/site and compose the
//! layered post-processing system prompt.
//!
//! # The layers
//! 1. **BASE** — the user's selected post-processing prompt (General/Email/Coding
//!    or a custom one). Always present; unchanged behavior.
//! 2. **CONTEXT (soft)** — an automatic, ≤2-line nudge derived from the detected
//!    app *category* (tone + vocabulary). Never restructures or hard-formats.
//!
//! HARD per-app formatting is no longer built in: it is what the App Modes
//! extension does, in its own transform hook and its own storage, so Grain
//! carries neither the setting nor the matcher for it.
//!
//! This is a **zero-overhead inline interceptor**, not a new engine: detection is
//! one cheap OS call made ONCE per finalized transcript (never per rolling chunk),
//! right before LLM post-processing, and composition is pure string work. When
//! context awareness is off — or nothing is detected — the base prompt is
//! returned untouched, so the common path is exactly today's.
//!
//! Detection is Windows-only for now; other platforms return `None`, degrading
//! cleanly to BASE-only behavior. Browser URL/site detection is a later increment
//! (needs UI Automation); until then browsers get the generic `Browser` category.

use grain_core::AppSettings;

/// Coarse app category driving the automatic SOFT context line. Deliberately a
/// small, robust bucket set (à la the incumbents' 4–8 categories) rather than a
/// per-app rule table: unknown apps fall to [`AppCategory::Other`], which adds no
/// context at all, so behavior degrades safely for the long tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCategory {
    /// Code editors and IDEs — technical vocabulary, keep jargon.
    Ide,
    /// Shells and terminals. Split from [`AppCategory::Ide`] because the output
    /// wanted is a *command*, not prose: no sentence casing, no trailing period.
    Terminal,
    /// Email composers — slightly polished, but NO email layout unless dictated.
    Email,
    /// Work chat (Slack/Teams/…): professional but concise and conversational.
    WorkChat,
    /// Personal messengers (WhatsApp/Messenger/…): keep the user's casual tone.
    PersonalChat,
    /// Social post composers (X/Reddit/…): casual, punchy, user's own voice.
    Social,
    /// Docs / notes editors (Notion/Docs/Word): readable prose, preserve structure.
    Docs,
    /// A pull-request or issue body: prose mixed with identifiers, and markdown.
    /// Neither [`AppCategory::Ide`] (not code) nor [`AppCategory::Docs`] (not
    /// prose) fits it.
    CodeReview,
    /// A prompt box for an AI assistant. The user is writing an *instruction*, so
    /// the usual "polish it" reflex is actively harmful — see [`soft_line`].
    AiChat,
    /// An issue tracker (Jira/Linear/Asana): imperative, factual, no narration.
    Ticket,
    /// A web browser whose site we could not resolve: tone-neutral light cleanup.
    Browser,
    /// Anything unrecognized — no soft context is added.
    Other,
}

impl AppCategory {
    /// The SOFT context line for this category, or `None` when nothing should be
    /// added (`Other`). Kept to ≤2 sentences and explicitly non-restructuring so
    /// it stays token-cheap and honors the "no hard formatting" constraint.
    ///
    /// These are deliberately terse. They ride on EVERY dictation once context
    /// awareness is on, so a wasted clause is a wasted clause a few thousand
    /// times a week; the earlier wording spent ~55 tokens on the IDE line alone.
    /// What must survive compression are the *negative* guards ("do not add",
    /// "unless dictated") — they are what keeps this layer soft, and dropping
    /// them would quietly turn tone hints into hard formatting.
    fn soft_line(self) -> Option<&'static str> {
        Some(match self {
            AppCategory::Ide => "A code editor. Keep identifiers and library names exactly as spoken (Tauri, useEffect); never 'correct' jargon into plain English. Terse; backtick code-like tokens.",
            AppCategory::Terminal => "A terminal. This is a command or path, not prose: no sentence casing, no trailing period, and keep flags, paths and casing verbatim.",
            AppCategory::Email => "An email composer. Slightly more polished and professional, but add no subject, greeting or sign-off, and impose no email layout unless dictated.",
            AppCategory::WorkChat => "Work chat (Slack/Teams). Professional but concise and conversational; add no greeting and do not restructure into formal paragraphs.",
            AppCategory::PersonalChat => "A casual messenger. Keep the user's own slang and phrasing; light cleanup only, never formalize.",
            AppCategory::Social => "A social post composer. Casual and punchy in the user's own voice; add no hashtags or emoji unless dictated.",
            AppCategory::Docs => "A document or notes editor. Readable prose is welcome, but preserve the user's wording and structure; impose no headings or lists unless dictated.",
            AppCategory::CodeReview => "A pull-request or issue box: prose mixed with code. Keep identifiers exact and backticked, stay direct, and do not pad.",
            // The one category whose instruction is "do LESS". A prompt is an
            // instruction the user is composing for another model, where
            // smoothing wording away is the exact failure mode: specifics are
            // the payload.
            AppCategory::AiChat => "A prompt box for an AI assistant. This is an instruction the user is writing, not prose to polish: preserve their exact intent, wording and specifics, and never soften, summarize or generalize it.",
            AppCategory::Ticket => "An issue tracker (Jira/Linear). Factual and imperative; do not narrate or add pleasantries.",
            // The fallback for an unresolved site, so it fires on the widest
            // range of unknown surfaces — which is exactly why it, of all the
            // lines, must say plainly that it does not restructure.
            AppCategory::Browser => "A text field in a web browser. Light, tone-neutral cleanup; match the style the user is already writing in and do not restructure.",
            AppCategory::Other => return None,
        })
    }
}

/// Address-bar host → category. **This is the table that makes context awareness
/// work at all for most people**: email, chat, docs and social overwhelmingly
/// live in a browser tab, and until this existed every one of them resolved to
/// the generic [`AppCategory::Browser`] line — the weakest bucket — even though
/// the host was already being read and then thrown away.
///
/// Ordered MOST SPECIFIC FIRST and matched in order, because [`host_matches`]
/// also accepts subdomains: `mail.google.com` would match a bare `google.com`
/// entry, so the specific row has to be seen first.
///
/// Hosts only, no paths, for now. Path scoping (`github.com/pulls` →
/// `CodeReview` while `github.com/docs` is not) needs the full URL, which
/// arrives with the focus-anchored resolver; this table is shaped so adding it
/// is a second column, not a rewrite.
static SITE_TABLE: &[(&str, AppCategory)] = &[
    // -- Email (webmail) --
    ("mail.google.com", AppCategory::Email),
    ("mail.proton.me", AppCategory::Email),
    ("outlook.office.com", AppCategory::Email),
    ("outlook.office365.com", AppCategory::Email),
    ("outlook.live.com", AppCategory::Email),
    ("mail.yahoo.com", AppCategory::Email),
    ("mail.zoho.com", AppCategory::Email),
    ("fastmail.com", AppCategory::Email),
    ("hey.com", AppCategory::Email),
    ("superhuman.com", AppCategory::Email),
    ("roundcube.", AppCategory::Email),
    // -- AI assistants (prompt boxes) --
    ("claude.ai", AppCategory::AiChat),
    ("chatgpt.com", AppCategory::AiChat),
    ("chat.openai.com", AppCategory::AiChat),
    ("gemini.google.com", AppCategory::AiChat),
    ("aistudio.google.com", AppCategory::AiChat),
    ("perplexity.ai", AppCategory::AiChat),
    ("poe.com", AppCategory::AiChat),
    ("copilot.microsoft.com", AppCategory::AiChat),
    ("chat.deepseek.com", AppCategory::AiChat),
    ("chat.mistral.ai", AppCategory::AiChat),
    ("grok.com", AppCategory::AiChat),
    ("t3.chat", AppCategory::AiChat),
    ("openrouter.ai", AppCategory::AiChat),
    // -- Code review / repo hosts. Writing into a text box on these is almost
    //    always a PR body, an issue, or a review comment.
    ("github.com", AppCategory::CodeReview),
    ("gitlab.com", AppCategory::CodeReview),
    ("bitbucket.org", AppCategory::CodeReview),
    ("codeberg.org", AppCategory::CodeReview),
    ("gerrit.", AppCategory::CodeReview),
    ("stackoverflow.com", AppCategory::CodeReview),
    // -- Issue trackers --
    ("atlassian.net", AppCategory::Ticket),
    ("jira.", AppCategory::Ticket),
    ("linear.app", AppCategory::Ticket),
    ("app.asana.com", AppCategory::Ticket),
    ("trello.com", AppCategory::Ticket),
    ("shortcut.com", AppCategory::Ticket),
    ("height.app", AppCategory::Ticket),
    ("monday.com", AppCategory::Ticket),
    ("clickup.com", AppCategory::Ticket),
    // -- Work chat --
    ("slack.com", AppCategory::WorkChat),
    ("teams.microsoft.com", AppCategory::WorkChat),
    ("teams.live.com", AppCategory::WorkChat),
    ("discord.com", AppCategory::WorkChat),
    ("chat.google.com", AppCategory::WorkChat),
    ("webex.com", AppCategory::WorkChat),
    ("zoom.us", AppCategory::WorkChat),
    // -- Personal messengers --
    ("web.whatsapp.com", AppCategory::PersonalChat),
    ("web.telegram.org", AppCategory::PersonalChat),
    ("messenger.com", AppCategory::PersonalChat),
    ("signal.org", AppCategory::PersonalChat),
    ("instagram.com", AppCategory::PersonalChat),
    // -- Social composers --
    ("x.com", AppCategory::Social),
    ("twitter.com", AppCategory::Social),
    ("reddit.com", AppCategory::Social),
    ("bsky.app", AppCategory::Social),
    ("threads.net", AppCategory::Social),
    ("mastodon.social", AppCategory::Social),
    ("linkedin.com", AppCategory::Social),
    ("news.ycombinator.com", AppCategory::Social),
    // -- Docs / notes / long-form --
    ("docs.google.com", AppCategory::Docs),
    ("notion.so", AppCategory::Docs),
    ("notion.site", AppCategory::Docs),
    ("coda.io", AppCategory::Docs),
    ("obsidian.md", AppCategory::Docs),
    ("roamresearch.com", AppCategory::Docs),
    ("workflowy.com", AppCategory::Docs),
    ("evernote.com", AppCategory::Docs),
    ("onenote.com", AppCategory::Docs),
    ("dropbox.com", AppCategory::Docs),
    ("medium.com", AppCategory::Docs),
    ("substack.com", AppCategory::Docs),
    ("ghost.io", AppCategory::Docs),
    ("wordpress.com", AppCategory::Docs),
    ("confluence.", AppCategory::Docs),
    ("sharepoint.com", AppCategory::Docs),
    ("quip.com", AppCategory::Docs),
    ("hackmd.io", AppCategory::Docs),
];

/// What kind of text field the caret is in. Derived from the focused element's
/// control type and flags — see `uia::focus_chain`.
///
/// This exists mainly for [`FieldKind::SingleLine`]. A large share of dictation
/// goes into search boxes and one-line inputs, where the pipeline's reflex to
/// capitalize the first word and add a full stop produces something the user
/// then has to delete. Knowing the field is one line is enough to stop that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldKind {
    /// A one-line input: search box, name field, URL bar, chat composer.
    SingleLine,
    /// A multi-line editor or document body.
    MultiLine,
    /// A password field. Nothing is ever read from one; recorded so callers can
    /// see the difference between "read nothing" and "refused to read".
    Password,
    /// Focus resolved, but the element says nothing useful about its shape.
    #[default]
    Unknown,
}

/// [GRAIN] Whether the focused element can actually receive pasted text.
///
/// Distinct from [`FieldKind`], which answers "what SHAPE is this field" for
/// prompt construction and maps `TextPattern` presence straight to `MultiLine`.
/// That mapping is wrong for this question: a rendered web page body exposes
/// `TextPattern` and is the single most common surface a dictation paste lands
/// on by mistake. Telling the two apart needs `IsReadOnly` and `TextEditPattern`.
///
/// Used by Paste Catch (`paste_catch`) to decide whether a paste is about to be
/// thrown away. The three-way split is the whole point: `Unknown` means *no
/// evidence*, and must never be treated as a miss — see [`classify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusTarget {
    /// Text pasted here will land.
    Editable,
    /// Positive evidence that it will not: a read-only value, a button, a
    /// caret-less document.
    NotEditable,
    /// Resolved nothing conclusive. Not a miss — just no evidence either way.
    #[default]
    Unknown,
}

/// The focused element's control type, reduced to the three cases [`classify`]
/// reasons about. Keeping the UIA ids on the Windows side of this boundary is
/// what lets the decision table be a pure function with tests that run anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlClass {
    /// `Edit` — a text input.
    Edit,
    /// `Document` — an editor body, or a rendered page. Ambiguous by itself.
    Document,
    /// A control that unambiguously takes no typed text (button, list item,
    /// scroll bar, ...).
    NonText,
    /// Anything else, including the container types (`Pane`, `Window`, `Group`,
    /// `Custom`) that Electron and canvas apps report for real editors. These
    /// must stay inconclusive rather than be called non-editable.
    #[default]
    Other,
}

/// One read of the focused element, as facts. Filled by `uia::read_focus_facts`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FocusFacts {
    pub is_password: bool,
    /// `TextEditPattern` is available. Only controls supporting text *editing*
    /// expose it, which makes it the one unambiguous positive signal here.
    pub has_text_edit_pattern: bool,
    /// `Some(is_read_only)` when `ValuePattern` is available.
    pub value_read_only: Option<bool>,
    pub control: ControlClass,
    /// `TextPattern` yielded a caret (or selection) to anchor on.
    pub has_caret: bool,
}

/// The decision table. Pure, so the whole matrix is unit-testable without COM.
///
/// The governing rule is asymmetric on purpose: **only report `NotEditable` on
/// positive evidence.** A false `NotEditable` suppresses a paste the user wanted
/// and holds their clipboard behind an offer they did not need; a false
/// `Unknown` merely falls through to post-paste verification. So every
/// ambiguous case resolves to `Unknown`.
pub fn classify(facts: FocusFacts) -> FocusTarget {
    // A password box counts as landed and is never held: parking a password on
    // the clipboard behind a visible offer is a worse outcome than losing it.
    if facts.is_password {
        return FocusTarget::Editable;
    }
    if facts.has_text_edit_pattern {
        return FocusTarget::Editable;
    }
    // An explicit read-only flag is the clearest negative evidence available —
    // a disabled or display-only input that would silently swallow the paste.
    if let Some(read_only) = facts.value_read_only {
        return if read_only {
            FocusTarget::NotEditable
        } else {
            FocusTarget::Editable
        };
    }
    match facts.control {
        ControlClass::Edit => FocusTarget::Editable,
        // A document with no caret to anchor on is a read-only surface: a PDF
        // viewer, a mail preview, a rendered reader. WITH a caret it stays
        // ambiguous — a real editor and a selectable web page both present one,
        // and the web page is exactly the case we must not guess wrong about.
        ControlClass::Document if facts.has_caret => FocusTarget::Unknown,
        ControlClass::Document => FocusTarget::NotEditable,
        ControlClass::NonText => FocusTarget::NotEditable,
        ControlClass::Other => FocusTarget::Unknown,
    }
}

/// The text immediately around the caret, for seamless insertion.
///
/// Dictating into the middle of an existing sentence is where the pipeline is
/// most obviously wrong today: it capitalizes the first word and drops the
/// leading space, so the user fixes the same two things by hand every time. The
/// model can only get that right if it can see what it is landing between.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaretContext {
    /// Text immediately before the caret (tail-most [`MAX_CARET_CHARS`]).
    pub before: String,
    /// Text immediately after the caret (head-most [`MAX_CARET_CHARS`]).
    pub after: String,
}

impl CaretContext {
    fn is_empty(&self) -> bool {
        self.before.is_empty() && self.after.is_empty()
    }
}

/// How much text either side of the caret is worth sending.
///
/// Small on purpose. What the model needs is the *seam* — the few words it must
/// join onto and the punctuation it must not duplicate — not the document. This
/// is also the context-rot argument in miniature: surrounding prose is the most
/// confusable possible distractor for a task whose output is also prose, so the
/// budget stays at the joint.
const MAX_CARET_CHARS: usize = 320;

/// How much we trust the resolved surface.
///
/// The gating rule this exists for: **a wrong rule is worse than no rule.** A
/// mis-resolved surface that still applies its formatting silently rewrites the
/// user's email as a chat message, which is far worse than adding no context at
/// all. So low confidence degrades to less context, never to a guess applied at
/// full strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Confidence {
    /// Structural evidence: the URL came off the focused element's own Document
    /// ancestor, so it is the tab the caret is in and cannot be another tab.
    Exact,
    /// Heuristic evidence: window title, or the legacy address-bar scan.
    Probable,
    /// No evidence beyond the executable.
    #[default]
    Guess,
}

impl Confidence {
    /// Whether a *site*-derived category may be applied. Site resolution is the
    /// strongest claim this layer makes (it overrides the app), so it demands
    /// structural evidence; a `Probable` URL still names the site in the prompt
    /// but does not get to change the category.
    fn allows_site_category(self) -> bool {
        self == Confidence::Exact
    }
}

/// Resolve an address-bar host to a category, or `None` when the site is
/// unknown (the caller then keeps the generic [`AppCategory::Browser`]).
pub(crate) fn category_for_site(host: &str) -> Option<AppCategory> {
    let host = host.trim().trim_start_matches("www.");
    SITE_TABLE
        .iter()
        .find(|(pattern, _)| host_matches(host, pattern))
        .map(|(_, category)| *category)
}

/// Whether `host` IS `pattern` or is a subdomain of it.
///
/// The dot boundary is the whole point: a plain `ends_with` would match
/// `notgithub.com` against `github.com` and `evil-slack.com` against
/// `slack.com`, handing an attacker-chosen domain the tone of a trusted one.
/// Patterns ending in `.` are prefix wildcards instead (`jira.` matches
/// `jira.acme.com`), which is how self-hosted installs of Jira, Confluence,
/// Gerrit and Roundcube name themselves.
fn host_matches(host: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('.') {
        // `jira.` → matches `jira.acme.com`, but not `myjira.acme.com`.
        return host
            .split('.')
            .next()
            .is_some_and(|label| label == prefix);
    }
    if host == pattern {
        return true;
    }
    host.len() > pattern.len()
        && host.ends_with(pattern)
        && host.as_bytes()[host.len() - pattern.len() - 1] == b'.'
}

/// Map an executable stem (lowercased, no extension) to a coarse [`AppCategory`].
/// Covers the popular desktop apps; everything else is `Other` (no soft context).
/// Match is a substring/stem check so channel variants (`code`, `code - insiders`,
/// `WhatsApp`, `WhatsAppDesktop`) all resolve.
fn category_for_exe(stem: &str) -> AppCategory {
    // IDEs / editors. Terminals used to live here; they are their own category
    // now because they want a command, not a sentence.
    const IDE: &[&str] = &[
        "code",
        "cursor",
        "windsurf",
        "devenv",
        "idea64",
        "idea",
        "pycharm64",
        "pycharm",
        "webstorm64",
        "webstorm",
        "goland64",
        "clion64",
        "rider64",
        "rustrover64",
        "phpstorm64",
        "sublime_text",
        "zed",
        "nvim",
        "vim",
    ];
    // Shells and terminal emulators.
    const TERMINAL: &[&str] = &[
        "windowsterminal",
        "wt",
        "powershell",
        "pwsh",
        "cmd",
        "alacritty",
        "wezterm-gui",
        "wezterm",
        "kitty",
        "conemu",
        "hyper",
        "ghostty",
        "tabby",
    ];
    // Email clients.
    const EMAIL: &[&str] = &[
        "outlook",
        "thunderbird",
        "hmaildesktop",
        "mailspring",
        "spark",
    ];
    // Work chat.
    const WORK_CHAT: &[&str] = &["slack", "teams", "ms-teams", "webex", "discord"];
    // Personal messengers.
    const PERSONAL_CHAT: &[&str] = &[
        "whatsapp",
        "messenger",
        "telegram",
        "signal",
        "wechat",
        "line",
        "viber",
        "imessage",
    ];
    // Social composers (native desktop clients).
    const SOCIAL: &[&str] = &["x", "twitter", "tweetdeck"];
    // Docs / notes.
    const DOCS: &[&str] = &[
        "notion", "obsidian", "winword", "onenote", "evernote", "bear", "typora", "logseq",
    ];
    // Browsers — kept broad so URL/site awareness is browser-agnostic. Covers
    // Chromium forks and Gecko/Firefox forks; the URL reader itself works off the
    // accessibility tree, not a per-browser rule.
    const BROWSER: &[&str] = &[
        "chrome",
        "msedge",
        "firefox",
        "brave",
        "opera",
        "operagx",
        "vivaldi",
        "arc",
        "browser",
        "chromium",
        "zen",
        "librewolf",
        "waterfox",
        "floorp",
        "mullvad",
        "palemoon",
        "seamonkey",
        "thorium",
        "yandex",
        "maxthon",
        "midori",
        "epic",
        "min",
        "sidekick",
        "wavebox",
        "falkon",
        "qutebrowser",
        "ungoogled",
        "duckduckgo",
        "tor",
    ];

    // Short keys (≤3 chars, e.g. "wt", "zen", "arc", "tor", "min", "x") must match
    // the stem EXACTLY — substring-matching them would misfire on ordinary words
    // ("editor" contains "tor", "examine" contains "min"). Longer keys may match as
    // a substring so channel variants ("code - insiders", "whatsappdesktop") resolve.
    let hit = |set: &[&str]| {
        set.iter()
            .any(|k| stem == *k || (k.len() >= 4 && stem.contains(k)))
    };
    if hit(IDE) {
        AppCategory::Ide
    } else if hit(TERMINAL) {
        AppCategory::Terminal
    } else if hit(EMAIL) {
        AppCategory::Email
    } else if hit(WORK_CHAT) {
        AppCategory::WorkChat
    } else if hit(PERSONAL_CHAT) {
        AppCategory::PersonalChat
    } else if hit(SOCIAL) {
        AppCategory::Social
    } else if hit(DOCS) {
        AppCategory::Docs
    } else if hit(BROWSER) {
        AppCategory::Browser
    } else {
        AppCategory::Other
    }
}

/// Cap on how many nearby terms we forward — keeps the prompt bounded and the
/// hint genuinely "additive" rather than a dump.
const MAX_NEARBY_TERMS: usize = 12;
/// Cap on how much focused-field text we scan for terms (bounds cost on huge docs).
const MAX_SCAN_CHARS: usize = 4000;

/// A compact stop-list of the most common English words. Extraction drops any
/// lowercase token found here, so ordinary prose contributes nothing — only
/// genuinely unusual tokens (names, identifiers, jargon) survive. Kept small on
/// purpose: the shape heuristics in [`extract_unique_terms`] do the heavy lifting;
/// this only catches common *lowercase* words that would otherwise slip through.
const COMMON_WORDS: &[&str] = &[
    "the",
    "and",
    "you",
    "that",
    "was",
    "for",
    "are",
    "with",
    "his",
    "they",
    "this",
    "have",
    "from",
    "one",
    "had",
    "but",
    "not",
    "what",
    "all",
    "were",
    "when",
    "your",
    "can",
    "said",
    "there",
    "use",
    "each",
    "which",
    "she",
    "how",
    "their",
    "will",
    "other",
    "about",
    "out",
    "many",
    "then",
    "them",
    "these",
    "some",
    "her",
    "would",
    "make",
    "like",
    "him",
    "into",
    "time",
    "has",
    "look",
    "two",
    "more",
    "write",
    "see",
    "number",
    "way",
    "could",
    "people",
    "than",
    "first",
    "water",
    "been",
    "call",
    "who",
    "its",
    "now",
    "find",
    "long",
    "down",
    "day",
    "did",
    "get",
    "come",
    "made",
    "may",
    "part",
    "over",
    "new",
    "sound",
    "take",
    "only",
    "little",
    "work",
    "know",
    "place",
    "year",
    "live",
    "back",
    "give",
    "most",
    "very",
    "after",
    "thing",
    "our",
    "just",
    "name",
    "good",
    "sentence",
    "man",
    "think",
    "say",
    "great",
    "where",
    "help",
    "through",
    "much",
    "before",
    "line",
    "right",
    "too",
    "mean",
    "old",
    "any",
    "same",
    "tell",
    "boy",
    "follow",
    "came",
    "want",
    "show",
    "also",
    "around",
    "form",
    "three",
    "small",
    "set",
    "put",
    "end",
    "does",
    "another",
    "well",
    "large",
    "must",
    "big",
    "even",
    "such",
    "because",
    "turn",
    "here",
    "why",
    "ask",
    "went",
    "men",
    "read",
    "need",
    "land",
    "different",
    "home",
    "move",
    "try",
    "kind",
    "hand",
    "picture",
    "again",
    "change",
    "off",
    "play",
    "spell",
    "air",
    "away",
    "animal",
    "house",
    "point",
    "page",
    "letter",
    "mother",
    "answer",
    "found",
    "study",
    "still",
    "learn",
    "should",
    "america",
    "world",
    "high",
    "every",
    "near",
    "add",
    "food",
    "between",
    "own",
    "below",
    "country",
    "plant",
    "last",
    "school",
    "father",
    "keep",
    "tree",
    "never",
    "start",
    "city",
    "earth",
    "eye",
    "light",
    "thought",
    "head",
    "under",
    "story",
    "saw",
    "left",
    "few",
    "while",
    "along",
    "might",
    "close",
    "something",
    "seem",
    "next",
    "hard",
    "open",
    "example",
    "begin",
    "life",
    "always",
    "those",
    "both",
    "paper",
    "together",
    "got",
    "group",
    "often",
    "run",
    "important",
    "until",
    "children",
    "side",
    "feet",
    "car",
    "mile",
    "night",
    "walk",
    "white",
    "sea",
    "began",
    "grow",
    "took",
    "river",
    "four",
    "carry",
    "state",
    "once",
    "book",
    "hear",
    "stop",
    "without",
    "second",
    "later",
    "miss",
    "idea",
    "enough",
    "eat",
    "face",
    "watch",
    "far",
    "really",
    "almost",
    "let",
    "above",
    "girl",
    "sometimes",
    "mountain",
    "cut",
    "young",
    "talk",
    "soon",
    "list",
    "song",
    "being",
    "leave",
    "family",
    "it's",
    "please",
    "thanks",
    "hey",
    "hi",
    "yeah",
    "okay",
    "just",
    "going",
    "really",
    "actually",
    "basically",
];

/// Extract UNIQUE, non-dictionary tokens worth biasing the LLM with — the
/// additive hint the user asked for (proper nouns like `Rita`/`Google`, and
/// identifiers/libraries like `useGrainStore`, `snake_case`, `PyTorch`), NOT raw
/// prose. A token is kept when it "looks intentional":
///   * has an internal capital (camelCase / PascalCase), or
///   * contains `_` or a digit (identifiers/versions), or
///   * is Capitalized (a likely proper noun), or
///   * is an ALL-CAPS acronym (≥2 chars),
/// and it is not an ordinary lowercase English word (checked against
/// [`COMMON_WORDS`]). De-duplicated case-insensitively, first-seen casing kept,
/// capped at [`MAX_NEARBY_TERMS`]. This is what makes it *reduce* hallucination:
/// we never pass gaps or partial sentences, only high-signal names.
pub fn extract_unique_terms(text: &str) -> Vec<String> {
    let text: String = text.chars().take(MAX_SCAN_CHARS).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    // Tokens are runs of letters/digits/underscore (identifier-ish).
    for tok in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        let tok = tok.trim_matches('_');
        if tok.chars().count() < 3 || tok.chars().count() > 40 {
            continue;
        }
        let lower = tok.to_ascii_lowercase();
        if COMMON_WORDS.contains(&lower.as_str()) {
            continue;
        }

        let chars: Vec<char> = tok.chars().collect();
        let first_upper = chars[0].is_uppercase();
        let has_underscore = tok.contains('_');
        let has_digit = chars.iter().any(|c| c.is_ascii_digit());
        let internal_upper = chars.iter().skip(1).any(|c| c.is_uppercase());
        let all_upper = chars.iter().all(|c| c.is_uppercase() || c.is_ascii_digit())
            && chars.iter().any(|c| c.is_uppercase());
        // Plain lowercase words with no distinguishing shape are ordinary prose —
        // skip them even if they dodged the stop-list, to stay high-signal.
        let intentional = internal_upper || has_underscore || has_digit || first_upper || all_upper;
        if !intentional {
            continue;
        }

        if seen.insert(lower) {
            out.push(tok.to_string());
            if out.len() >= MAX_NEARBY_TERMS {
                break;
            }
        }
    }
    out
}

/// A snapshot of the foreground target, taken right before post-processing. The
/// paste target keeps focus while Grain runs in the background, so the foreground
/// window IS the app the text is about to land in.
#[derive(Debug, Clone)]
pub struct ActiveContext {
    /// Human-facing app name for the prompt/UI (window title or exe stem).
    pub app_name: String,
    /// Executable stem, lowercased, no extension — the process-matching key.
    pub exe: String,
    /// Full executable path, when resolvable. Unlike `exe` (a stem for *matching*),
    /// this is a *launchable* path — handed to extensions that need an app they
    /// can actually open. Empty when unavailable.
    pub exe_path: String,
    pub category: AppCategory,
    /// Browser address-bar host, when the foreground app is a browser and UI
    /// Automation resolved it (e.g. `mail.google.com`). `None` otherwise.
    pub url_host: Option<String>,
    /// The shape of the field the caret is in. Drives the one-line punctuation
    /// suppression — see [`FieldKind::SingleLine`].
    pub field: FieldKind,
    /// The ARIA landmark of the nearest enclosing region ("main",
    /// "complementary", …), when the surface exposes one.
    pub region: Option<String>,
    /// How much of the above is structural evidence rather than a heuristic.
    /// Gates whether the site may override the app category.
    pub confidence: Confidence,
    /// Text either side of the caret, when the seamless-insertion opt-in is on
    /// and the surface exposes a caret. Ephemeral: never stored, never logged.
    pub caret: Option<CaretContext>,
    /// Unique non-dictionary tokens read from the focused field (proper nouns,
    /// identifiers, library names) — an ADDITIVE bias hint, never raw text. Empty
    /// unless the nearby-terms opt-in is on and something was found.
    pub nearby_terms: Vec<String>,
}

/// Compose the final post-processing system prompt from up to four stages.
///
/// `spoken_instruction` is the **Prompt Record** layer: an instruction the user
/// dictated mid-recording (by clicking the pill), aimed at THIS transcript. It is
/// the absolute highest authority, and is applied even when context awareness is
/// off.
///
/// Returns `base` unchanged when nothing applies (no spoken instruction, context
/// off / no detection), so the common path is byte-for-byte today's behavior. Otherwise a compact preamble is prepended (NOT appended — so
/// it precedes the transcript in both the structured and legacy `${output}`
/// paths) framing the layers and their priority.
pub fn compose_prompt(
    base: &str,
    settings: &AppSettings,
    ctx: Option<&ActiveContext>,
    spoken_instruction: Option<&str>,
) -> String {
    let spoken = spoken_instruction
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    // Context-awareness layers only when the feature is on AND a target was
    // detected. The spoken instruction is independent of that toggle.
    let ctx = if settings.context_awareness_enabled {
        ctx
    } else {
        None
    };
    let soft = ctx.and_then(|c| c.category.soft_line());
    let terms: &[String] = ctx.map(|c| c.nearby_terms.as_slice()).unwrap_or(&[]);
    // A one-line field gets one extra clause, because the pipeline's habit of
    // capitalizing and adding a full stop is wrong in a search box and the user
    // has to delete it every time.
    let one_line = ctx.is_some_and(|c| c.field == FieldKind::SingleLine);
    let caret = ctx.and_then(|c| c.caret.as_ref());
    let has_ctx = soft.is_some() || !terms.is_empty() || one_line || caret.is_some();

    if spoken.is_none() && !has_ctx {
        // The quiet case that most looks like a bug: the feature is on,
        // detection may even have succeeded, and the prompt still goes out
        // untouched — because the app resolved to `Other`, which adds nothing by
        // design. Said out loud, but only to someone who opted in.
        if settings.context_awareness_enabled {
            log::info!("[GRAIN] context: prompt unchanged — nothing to add for this surface");
        }
        return base.to_string();
    }

    let mut pre = String::with_capacity(base.len() + 640);

    // 1) Spoken instruction — ABSOLUTE highest priority. The user just dictated it
    // for this exact transcript, so it outranks every rule below. It is an
    // instruction ABOUT the transcript, never content to emit.
    if let Some(instr) = spoken {
        pre.push_str("[Spoken instruction — HIGHEST PRIORITY]\n");
        pre.push_str(
            "The user just dictated this instruction for how to transform the \
             transcript. Treat it as the top authority, above every rule below \
             (including any app-specific formatting). Apply it to the transcript; \
             never output the instruction text itself:\n",
        );
        pre.push_str(instr);
        pre.push_str("\n\n");
    }

    // 2) Context-awareness block (soft context / nearby terms).
    if has_ctx {
        if let Some(c) = ctx {
            pre.push_str("[Context awareness]\n");
            pre.push_str(&format!(
                "The user is dictating into \"{}\".",
                c.app_name.trim()
            ));
            // Only assert the site when the URL came structurally off the
            // focused element's own Document ancestor. A `Probable` host was
            // found by scanning the window for something URL-shaped, which on a
            // multi-tab or split-view window can belong to a tab the user is NOT
            // typing into — and naming the wrong site is worse than naming none.
            // It stays on `ActiveContext` either way, with its confidence, so
            // callers that want it (the extension API) can judge for themselves.
            if let Some(host) = c
                .url_host
                .as_deref()
                .filter(|_| c.confidence == Confidence::Exact)
            {
                pre.push_str(&format!(" (website: {host})"));
            }
            // Only worth a few tokens when it actually disambiguates: "main"
            // says nothing a prompt can act on, but a sidebar or a dialog does.
            if let Some(region) = c.region.as_deref().filter(|r| *r != "main") {
                pre.push_str(&format!(" (page region: {region})"));
            }
            pre.push('\n');
        }
        if let Some(line) = soft {
            pre.push_str("Soft context (tone/vocabulary only, never restructure): ");
            pre.push_str(line);
            pre.push('\n');
        }
        if one_line {
            pre.push_str(
                "The target is a SINGLE-LINE field (a search or entry box): output one \
                 line, and do not add a trailing period or sentence-case it unless the \
                 user dictated it that way.\n",
            );
        }
        // The seam block. Two things had to change after it leaked verbatim into
        // a user's email draft:
        //
        // 1. The excerpts are no longer wrapped in `<before_text>` /
        //    `<after_text>`. XML-ish tags around a block of prose read as
        //    "content to emit" to a model that is already being asked to return
        //    text, and that is exactly what it did — tags included.
        // 2. The prohibition now comes AFTER the excerpts as well as before. The
        //    last thing read about the excerpts is that they are not the input.
        //
        // The output-format guard appended at the very end of the whole prompt
        // (see below) is the third layer, and the only one that does not depend
        // on the model choosing to comply with prose.
        if let Some(caret) = caret {
            pre.push_str(
                "Reference only — the text already around the cursor, NOT input, NOT \
                 to be repeated:\n",
            );
            if !caret.before.is_empty() {
                pre.push_str("  …immediately before: ");
                pre.push_str(caret.before.trim());
                pre.push('\n');
            }
            if !caret.after.is_empty() {
                pre.push_str("  …immediately after: ");
                pre.push_str(caret.after.trim());
                pre.push('\n');
            }
            pre.push_str(
                "Use those two ONLY to make the transcript join cleanly: correct \
                 leading/trailing spacing, continue mid-sentence without \
                 re-capitalizing, and do not repeat punctuation already there. Never \
                 output any part of them.\n",
            );
        }
        if !terms.is_empty() {
            // Additive, LOW authority: only fix a term to one of these spellings when
            // the transcript clearly meant it; otherwise ignore. Never insert them.
            pre.push_str(
                "Nearby terms the user may be referring to — use ONLY to correct the \
                 spelling of a word already in the transcript (proper nouns, code \
                 identifiers, library names); do NOT insert any that were not spoken: ",
            );
            pre.push_str(&terms.join(", "));
            pre.push('\n');
        }
        pre.push_str(
            "Apply the above as guidance over the cleanup rules below. Priority when \
             instructions conflict: the spoken instruction first, then the base cleanup \
             rules, then soft context. Keep edits minimal, preserve meaning, and never \
             invent content that was not dictated.\n\n",
        );
    }

    // [GRAIN] What actually reached the model. Detection succeeding and the
    // prompt changing are different questions — context awareness can be off,
    // or the category can be `Other`, and detection will still have logged a
    // confident-looking result while the prompt went out untouched. This is the
    // line that closes that gap. Layer names only; no prompt text.
    let mut layers: Vec<&str> = Vec::new();
    if spoken.is_some() {
        layers.push("spoken");
    }
    if soft.is_some() {
        layers.push("soft");
    }
    if one_line {
        layers.push("one-line");
    }
    if caret.is_some() {
        layers.push("caret");
    }
    if !terms.is_empty() {
        layers.push("terms");
    }
    log::info!(
        "[GRAIN] context: prompt layers applied: {} (+{} bytes)",
        if layers.is_empty() {
            "none".to_string()
        } else {
            layers.join("+")
        },
        pre.len(),
    );

    pre.push_str(base);

    // [GRAIN] Terminal output constraint, and the reason it is LAST.
    //
    // Everything this function prepends is reference material — the app name,
    // the site, the text around the cursor — and every piece of it is a thing a
    // model can mistake for content to return. One did: a user's email draft
    // received the surrounding text and the seam instructions verbatim.
    //
    // Prose telling the model not to do that sits at the TOP of the prompt,
    // which is the least-attended position. This sits at the very end, after
    // the user's own prompt, which is the most-attended one. It is added only
    // when a context layer was actually applied, so a plain dictation with no
    // context is byte-for-byte what it was before.
    if has_ctx {
        pre.push_str(
            "\n\nOutput ONLY the corrected transcript itself — no surrounding text, \
             no labels, no notes, no explanation.",
        );
    }
    pre
}

/// Detect the foreground app/site. `None` on unsupported platforms or on any
/// failure (caller then falls back to BASE-only). Cheap: one Win32 round-trip for
/// the app; UI Automation is consulted only for browser URLs and for whichever
/// of the two content opt-ins (`read_nearby_terms`, `read_caret`) are on.
pub fn detect_active_context(read_nearby_terms: bool, read_caret: bool) -> Option<ActiveContext> {
    #[cfg(windows)]
    {
        windows_impl::detect(read_nearby_terms, read_caret)
    }
    #[cfg(not(windows))]
    {
        let _ = (read_nearby_terms, read_caret);
        None
    }
}

/// Cap on the text harvested from a whole window. Big enough for an email
/// thread or an article, small enough that it cannot dominate a model's context.
pub const MAX_WINDOW_TEXT_CHARS: usize = 4000;

/// Read the visible text of the foreground window from its accessibility tree.
///
/// # Why not a screenshot
///
/// This is what "read my screen" costs elsewhere in this market: a screen
/// recording permission, a frame encoded and shipped to a vision model, and in
/// one competitor's case a public trust incident. The accessibility tree already
/// holds the text, structured, with no permission prompt on Windows, no image
/// ever created, and no other application in frame — only the window the user is
/// actually typing into.
///
/// It genuinely misses things a screenshot would catch (canvas, video, scanned
/// PDFs, Figma). That is what the image path is for; this is the one that should
/// be reached for first.
///
/// `None` on unsupported platforms or when nothing readable was found. Password
/// fields are skipped, as everywhere else.
pub fn read_window_text() -> Option<String> {
    #[cfg(windows)]
    {
        uia::read_window_text()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Read the currently focused editable field's full text via UI Automation.
/// Used by the Agent (field context at summon) and by context bias. `None` on
/// unsupported platforms, password fields, or any failure. Silent — no UI.
pub fn read_focused_text() -> Option<String> {
    #[cfg(windows)]
    {
        uia::read_focused_value()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{category_for_exe, ActiveContext, AppCategory, Confidence};
    use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    pub(super) fn detect(read_nearby_terms: bool, read_caret: bool) -> Option<ActiveContext> {
        unsafe {
            // Each of these bails to BASE-only behavior. They used to bail
            // SILENTLY, which made "the feature did nothing" and "the feature is
            // off" produce identical evidence — the one distinction anyone
            // diagnosing this actually needs.
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                log::info!("[GRAIN] context: no context — no foreground window");
                return None;
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                log::info!("[GRAIN] context: no context — foreground window has no process");
                return None;
            }

            let Some(exe_path) = process_image_path(pid) else {
                // Usually an elevated target: an unelevated Grain cannot open a
                // handle to it. Worth naming, because the fix is a user action
                // (run Grain as admin) rather than a bug.
                log::info!(
                    "[GRAIN] context: no context — cannot read process {pid} \
                     (elevated target?)"
                );
                return None;
            };
            // Stem = file name without extension, lowercased.
            let exe = std::path::Path::new(&exe_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if exe.is_empty() {
                log::info!("[GRAIN] context: no context — unnamed executable");
                return None;
            }

            let category = category_for_exe(&exe);
            let app_name = window_title(hwnd).unwrap_or_else(|| exe.clone());

            // UI Automation is only worth spinning up when we actually need it:
            // a browser (for the URL) or the nearby-terms opt-in. Everything here
            // is best-effort and SILENT — any failure just yields None/empty.
            let is_browser = category == AppCategory::Browser;
            let scan = if is_browser || read_nearby_terms || read_caret {
                super::uia::read(hwnd, is_browser, read_nearby_terms, read_caret)
            } else {
                // Neither the URL nor the terms are wanted, so UI Automation is
                // never spun up at all — the common non-browser path costs one
                // Win32 round-trip and nothing else.
                Default::default()
            };

            // [GRAIN] Site beats app. The host was already being resolved and
            // then used only as a display string, so Gmail in Chrome got the
            // generic Browser line instead of the Email one — and since most
            // people's mail, chat, docs and social all live in a tab, the
            // majority of real dictation landed in the weakest bucket.
            //
            // Gated on confidence: only a URL taken structurally from the focused
            // element's own Document ancestor may override the category. A
            // `Probable` host (window title, or the legacy address-bar scan)
            // still names the site in the prompt but does not get to change how
            // the text is treated — a wrong rule is worse than no rule, and this
            // is where that principle is enforced.
            //
            // An unknown site keeps `category` as-is, which for a browser is
            // `Browser`: refining is strictly additive, never a downgrade.
            let app_category = category;
            let category = match scan.url_host.as_deref() {
                Some(host) if scan.confidence.allows_site_category() => {
                    super::category_for_site(host).unwrap_or(category)
                }
                _ => category,
            };

            // [GRAIN] One line saying what resolution concluded AND why.
            //
            // UI Automation is an external surface that varies by app, browser
            // and version, and every read in the scan degrades silently by
            // design. That is correct behavior and it is also why this line has
            // to exist: without it, "resolved nothing" and "switched off" leave
            // identical evidence.
            //
            // At INFO rather than DEBUG deliberately. Context awareness is
            // opt-in and off by default, so this costs a line per dictation
            // only for someone who turned it on — and that is exactly the person
            // asking whether it works. Making them first discover a log-level
            // setting to answer that would defeat the purpose.
            //
            // Never logs content: shapes, counts and decisions only, so turning
            // logging up can never become a transcript of what was typed. The
            // host is the one identifier included, because "did site detection
            // work" is unanswerable without it.
            let site_note = match (&scan.url_host, scan.confidence) {
                (Some(_), Confidence::Exact) if category != app_category => "site→category",
                (Some(_), Confidence::Exact) => "site known, no rule",
                // The distinction that matters most when a browser looks wrong:
                // the URL was found by scanning rather than structurally, so it
                // is deliberately NOT trusted to change anything.
                (Some(_), _) => "site untrusted (scan fallback), category unchanged",
                (None, _) => "no site",
            };
            log::info!(
                "[GRAIN] context: {exe} → {category:?} [{site_note}] | field={:?} \
                 confidence={:?} url={}({}) region={} | caret={} terms={}",
                scan.field,
                scan.confidence,
                scan.url_host.as_deref().unwrap_or("-"),
                scan.url_source,
                scan.region.as_deref().unwrap_or("-"),
                scan.caret
                    .as_ref()
                    .map(|c| format!("{}b/{}b", c.before.len(), c.after.len()))
                    .unwrap_or_else(|| if read_caret { "none".into() } else { "off".into() }),
                if read_nearby_terms {
                    scan.terms.len().to_string()
                } else {
                    "off".to_string()
                },
            );

            Some(ActiveContext {
                app_name,
                exe,
                exe_path,
                category,
                url_host: scan.url_host,
                field: scan.field,
                region: scan.region,
                confidence: scan.confidence,
                caret: scan.caret,
                nearby_terms: scan.terms,
            })
        }
    }

    /// Full image path of `pid` via `QueryFullProcessImageNameW`, which works with
    /// the limited-info access right (no elevation needed for most apps).
    unsafe fn process_image_path(pid: u32) -> Option<String> {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        res.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }

    /// The foreground window, or `None` when there isn't one. Shared with the
    /// standalone window-text reader, which has no scan of its own to ride on.
    pub(super) fn foreground_window() -> Option<HWND> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.0.is_null()).then_some(hwnd)
    }

    /// The foreground window's title (for display + UWP fallback), if non-empty.
    unsafe fn window_title(hwnd: HWND) -> Option<String> {
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        let title = title.trim();
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    }
}

/// [GRAIN] UI-Automation reads: browser URL host + focused-field unique terms.
/// Everything here is **best-effort and SILENT** — every call swallows failure
/// into `None`/empty, and password fields are never read. No UI is ever shown.
#[cfg(windows)]
mod uia {
    use super::{
        extract_unique_terms, host_from_url, CaretContext, Confidence, ControlClass, FieldKind,
        FocusFacts, MAX_CARET_CHARS,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextEditPattern,
        IUIAutomationTextPattern, IUIAutomationValuePattern, TreeScope_Descendants,
        TreeScope_Element, UIA_AriaRolePropertyId, UIA_ButtonControlTypeId,
        UIA_CalendarControlTypeId, UIA_CheckBoxControlTypeId, UIA_ControlTypePropertyId,
        UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_HeaderControlTypeId,
        UIA_HeaderItemControlTypeId, UIA_HyperlinkControlTypeId, UIA_ImageControlTypeId,
        UIA_ListItemControlTypeId, UIA_MenuBarControlTypeId, UIA_MenuItemControlTypeId,
        UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
        UIA_SeparatorControlTypeId, UIA_SliderControlTypeId, UIA_SplitButtonControlTypeId,
        UIA_StatusBarControlTypeId, UIA_TabItemControlTypeId, UIA_TextControlTypeId,
        UIA_TextEditPatternId, UIA_TextPatternId, UIA_ThumbControlTypeId, UIA_TitleBarControlTypeId,
        UIA_TreeItemControlTypeId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::Accessibility::{
        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    };

    /// Cap on focused-field text scanned (bounds cost on huge documents).
    const MAX_TEXT_CHARS: usize = 8000;

    /// Upper bound on `Edit` controls inspected when hunting the address bar —
    /// keeps a page full of inputs from making URL detection expensive.
    const MAX_EDIT_SCAN: i32 = 60;

    /// Upper bound on `Document` elements inspected. A browser window has one
    /// per rendered tab (plus the odd embedded frame), so this is generous;
    /// it only exists so a pathological page cannot make the scan unbounded.
    const MAX_DOCUMENT_SCAN: i32 = 16;

    /// Upper bound on elements inspected per control type when harvesting a
    /// whole window's text. A busy page has thousands; the character cap would
    /// stop us anyway, but this bounds the tree traversal itself.
    const MAX_WINDOW_ELEMENT_SCAN: i32 = 400;

    /// RAII COM init: balances a successful `CoInitializeEx` with `CoUninitialize`.
    /// If the thread was already in a different apartment (`RPC_E_CHANGED_MODE`),
    /// COM is still usable and we leave it alone.
    struct ComGuard(bool);
    impl ComGuard {
        unsafe fn init() -> Self {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            ComGuard(hr.is_ok())
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Everything one focus-anchored scan yields. Every field is best-effort and
    /// degrades to its default rather than failing the scan.
    pub(super) struct FocusScan {
        pub url_host: Option<String>,
        pub field: FieldKind,
        pub region: Option<String>,
        pub confidence: Confidence,
        pub terms: Vec<String>,
        pub caret: Option<CaretContext>,
        /// Which rung of the URL ladder produced the host. Logged, so a browser
        /// that resolves badly can be diagnosed without guessing at which
        /// mechanism failed.
        pub url_source: &'static str,
    }

    impl Default for FocusScan {
        fn default() -> Self {
            Self {
                url_host: None,
                field: FieldKind::default(),
                region: None,
                confidence: Confidence::default(),
                terms: Vec::new(),
                caret: None,
                url_source: "not-attempted",
            }
        }
    }

    /// Resolve the page URL, best evidence first.
    ///
    /// Browsers disagree about how they expose this, so one mechanism is not
    /// enough — Chromium answers on the first rung and Gecko does not.
    ///
    /// 1. **Focus chain.** An ancestor of the focused element carrying a
    ///    host-shaped value. Strongest: an ancestor of the caret belongs to the
    ///    caret's own tab, so split views and background tabs cannot intrude.
    /// 2. **The one visible document.** Gecko does not reliably present the page
    ///    root the way the walk expects, so look for `Document` elements
    ///    directly. If exactly ONE is on screen, it is unambiguously the tab
    ///    being looked at — background tabs are either absent from the tree or
    ///    marked offscreen — and that uniqueness is what earns it `Exact`.
    /// 3. **Several visible documents** (a split view): take the first, but
    ///    demote to `Probable`, because "first" is not "the one with the caret".
    /// 4. **Address-bar scan.** The old mechanism, kept last. It reads whatever
    ///    looks like a URL anywhere in the window, which can be a different tab
    ///    or a page input, so it never earns better than `Probable`.
    ///
    /// Only `Exact` may change the app category (see
    /// [`Confidence::allows_site_category`]), so the weak rungs still name the
    /// site without being trusted to act on it.
    unsafe fn resolve_url(
        automation: &IUIAutomation,
        hwnd: HWND,
        from_focus_chain: Option<&str>,
    ) -> (Option<String>, Confidence, &'static str) {
        if let Some(host) = from_focus_chain.and_then(host_from_url) {
            return (Some(host), Confidence::Exact, "focus-chain");
        }

        match visible_document_urls(automation, hwnd).as_slice() {
            [] => {}
            [only] => {
                if let Some(host) = host_from_url(only) {
                    return (Some(host), Confidence::Exact, "sole-document");
                }
            }
            many => {
                if let Some(host) = many.iter().find_map(|u| host_from_url(u)) {
                    return (Some(host), Confidence::Probable, "multi-document");
                }
            }
        }

        match read_url(automation, hwnd) {
            Some(host) => (Some(host), Confidence::Probable, "address-bar-scan"),
            None => {
                // Every rung failed. The two causes need different fixes and
                // look identical from outside, so count what the tree actually
                // contained: zero of everything means the browser never built an
                // accessibility tree for us (Gecko instantiates lazily), while
                // elements-but-no-URL means the tree is there and the URL simply
                // is not where we looked.
                log::debug!(
                    "[GRAIN] context: no URL on any rung — tree had {} document(s), {} edit(s)",
                    count_descendants(automation, hwnd, UIA_DocumentControlTypeId.0),
                    count_descendants(automation, hwnd, UIA_EditControlTypeId.0),
                );
                (None, Confidence::Guess, "none")
            }
        }
    }

    /// How many descendants of the window have a given control type. Diagnostic
    /// only, and only ever run on the path where everything else already failed.
    unsafe fn count_descendants(automation: &IUIAutomation, hwnd: HWND, control_type: i32) -> usize {
        descendants_of_type(automation, hwnd, control_type, i32::MAX).len()
    }

    /// Every descendant of `hwnd` with a given control type, capped.
    ///
    /// The one place that builds a window-scoped `FindAll`. Four callers used to
    /// repeat this preamble — resolve the root, build a property condition, run
    /// the scan, bound the result — with four chances to forget the bound.
    unsafe fn descendants_of_type(
        automation: &IUIAutomation,
        hwnd: HWND,
        control_type: i32,
        cap: i32,
    ) -> Vec<IUIAutomationElement> {
        let Ok(root) = automation.ElementFromHandle(hwnd) else {
            return Vec::new();
        };
        let Ok(condition) = automation
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(control_type))
        else {
            return Vec::new();
        };
        let Ok(found) = root.FindAll(TreeScope_Descendants, &condition) else {
            return Vec::new();
        };
        let len = found.Length().unwrap_or(0).min(cap);
        (0..len).filter_map(|i| found.GetElement(i).ok()).collect()
    }

    /// Values of every on-screen `Document` in the window.
    ///
    /// Offscreen ones are dropped because that is how a browser represents a
    /// background tab that still has an accessibility tree; keeping them would
    /// make "which tab am I in" ambiguous exactly when it matters.
    unsafe fn visible_document_urls(automation: &IUIAutomation, hwnd: HWND) -> Vec<String> {
        descendants_of_type(
            automation,
            hwnd,
            UIA_DocumentControlTypeId.0,
            MAX_DOCUMENT_SCAN,
        )
        .into_iter()
        .filter(|d| d.CurrentIsOffscreen().map(|b| b.as_bool()) != Ok(true))
        .filter_map(|d| read_value(&d))
        .collect()
    }

    /// How far up the ancestor chain to walk before giving up. Real chains from a
    /// text box to its document are 5–15 hops; the cap only stops a pathological
    /// tree from turning a bounded read into an unbounded one.
    const MAX_ANCESTOR_HOPS: usize = 24;

    /// Scan the focused element and its ancestors.
    ///
    /// # Why this walks UP instead of searching DOWN
    ///
    /// The old path started at the window root and ran
    /// `FindAll(TreeScope_Descendants, Edit)`, then guessed which of up to 60
    /// results was the address bar. That is a search that guesses, it is the
    /// exact pattern Microsoft documents as the UIA performance anti-pattern,
    /// and on a page full of inputs it can pick the wrong one.
    ///
    /// Grain does not need to understand the page. It needs one thing: where is
    /// the text about to land? That is never ambiguous —
    /// `GetFocusedElement` returns exactly the element that will receive the
    /// keystrokes. Everything else falls out of its ancestor chain:
    ///
    /// - the element itself → [`FieldKind`]
    /// - the first ancestor carrying an ARIA role → which region of the page
    ///   (sidebar vs main vs a card), since Chromium maps ARIA landmarks through
    /// - the first `Document` ancestor → **the tab the caret is in.** A tab you
    ///   are not typing into cannot be an ancestor of the element you are typing
    ///   into, so split views and multi-tab windows disambiguate for free.
    ///
    /// It is also cheaper than what it replaces: one upward walk of ~15 hops
    /// instead of a subtree sweep, with every property for each hop fetched in a
    /// single cross-process call via the cache request.
    pub(super) fn read(
        hwnd: HWND,
        want_url: bool,
        want_terms: bool,
        want_caret: bool,
    ) -> FocusScan {
        unsafe {
            let _com = ComGuard::init();
            let automation: IUIAutomation =
                match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                    Ok(a) => a,
                    Err(_) => return FocusScan::default(),
                };

            let mut scan = FocusScan::default();

            // The focused element anchors everything. If it cannot be resolved we
            // know nothing structurally, and only then is the legacy scan worth
            // running.
            let Ok(focused) = automation.GetFocusedElement() else {
                if want_url {
                    // No anchor: fall back to the old address-bar hunt, marked
                    // `Probable` so it can name the site without being trusted
                    // enough to change the category.
                    scan.url_host = read_url(&automation, hwnd);
                    scan.confidence = if scan.url_host.is_some() {
                        Confidence::Probable
                    } else {
                        Confidence::Guess
                    };
                }
                return scan;
            };

            scan.field = field_kind(&focused);

            // A password field is never read, and never contributes terms. This
            // is checked before anything else touches its content.
            if scan.field == FieldKind::Password {
                return scan;
            }

            if want_terms {
                scan.terms = match read_text_content(&focused) {
                    Some(text) => extract_unique_terms(&text),
                    None => Vec::new(),
                };
            }

            if want_caret {
                scan.caret = read_caret(&focused).filter(|c| !c.is_empty());
            }

            let (url, region) = walk_ancestors(&automation, &focused, want_url);
            scan.region = region;

            if want_url {
                let (host, confidence, source) = resolve_url(&automation, hwnd, url.as_deref());
                scan.url_host = host;
                scan.confidence = confidence;
                scan.url_source = source;
            }

            scan
        }
    }

    /// Walk from `focused` up to the document root, collecting the page region
    /// and the containing document's URL.
    ///
    /// Properties for every hop come back in ONE cross-process call each, via a
    /// cache request — the documented fix for UIA's cost model, where fetching
    /// properties one at a time means one round-trip per property. `ControlView`
    /// rather than `RawView` because the raw tree carries an order of magnitude
    /// more structural noise for the same answer.
    unsafe fn walk_ancestors(
        automation: &IUIAutomation,
        focused: &IUIAutomationElement,
        want_url: bool,
    ) -> (Option<String>, Option<String>) {
        let Ok(cache) = automation.CreateCacheRequest() else {
            return (None, None);
        };
        let _ = cache.AddProperty(UIA_ControlTypePropertyId);
        let _ = cache.AddProperty(UIA_AriaRolePropertyId);
        let _ = cache.SetTreeScope(TreeScope_Element);

        let Ok(walker) = automation.ControlViewWalker() else {
            return (None, None);
        };

        let mut region: Option<String> = None;
        let mut url: Option<String> = None;
        let mut current: IUIAutomationElement = focused.clone();

        for _ in 0..MAX_ANCESTOR_HOPS {
            let Ok(parent) = walker.GetParentElementBuildCache(&current, &cache) else {
                break;
            };

            // The nearest ancestor with an ARIA role names the region the caret
            // is in — "complementary" for a sidebar, "main" for the body, "form"
            // for a card. First one wins: it is the tightest enclosing scope.
            if region.is_none() {
                if let Ok(role) = parent.CachedAriaRole() {
                    let role = role.to_string().trim().to_ascii_lowercase();
                    if is_meaningful_region(&role) {
                        region = Some(role);
                    }
                }
            }

            // The tab's URL, from the chain that contains the caret.
            //
            // This deliberately does NOT require the ancestor to be a
            // `Document`. It used to, and that broke every Gecko-based browser:
            // Firefox and its forks (Zen, LibreWolf, Floorp, Waterfox) do not
            // reliably present the page root with that control type the way
            // Chromium does, so the walk climbed past the right element and
            // fell back to scanning the window — which is what made Gmail in
            // Zen resolve as a generic browser.
            //
            // What actually matters is not the control type but the structural
            // guarantee: this element is an ancestor of the focused one, so any
            // URL it carries belongs to the tab the caret is in and cannot be
            // another tab's. Reading the value of each ancestor and taking the
            // first that parses as a host keeps that guarantee and drops the
            // browser-engine assumption. `host_from_url` rejects anything not
            // host-shaped, so a non-URL value simply does not match.
            if want_url && url.is_none() {
                if let Some(value) = read_value(&parent) {
                    if host_from_url(&value).is_some() {
                        url = Some(value);
                        break; // found the tab this caret belongs to
                    }
                }
            }

            current = parent;
        }

        (url, region)
    }

    /// ARIA roles worth reporting as a region. Anything else (`generic`,
    /// `presentation`, an empty role) says nothing a prompt could use, and
    /// passing it on would spend tokens to describe a `<div>`.
    fn is_meaningful_region(role: &str) -> bool {
        matches!(
            role,
            "main"
                | "navigation"
                | "complementary"
                | "banner"
                | "contentinfo"
                | "form"
                | "search"
                | "article"
                | "dialog"
                | "region"
        )
    }

    /// Read a bounded span of text either side of the caret.
    ///
    /// Both ranges are built by taking the caret (or, if text is selected, the
    /// selection — dictation replaces it, so the seam is still its two edges),
    /// collapsing to that edge, and walking outward a fixed number of
    /// CHARACTERS. Walking outward rather than reading from the document start
    /// is what keeps this bounded on a large document: the alternative,
    /// `GetText` over a doc-anchored range, returns text from the top of the
    /// file, which is both expensive and not the text we need.
    unsafe fn read_caret(element: &IUIAutomationElement) -> Option<CaretContext> {
        let pattern: IUIAutomationTextPattern = element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()?;
        let selection = pattern.GetSelection().ok()?;
        // No selection array at all means no caret to anchor on (some read-only
        // surfaces). Nothing to do; not an error.
        let caret = selection.GetElement(0).ok()?;
        let span = MAX_CARET_CHARS as i32;

        // Before: collapse to the leading edge, then extend backwards.
        let before = caret
            .Clone()
            .ok()
            .and_then(|range| {
                range
                    .MoveEndpointByRange(
                        TextPatternRangeEndpoint_End,
                        &caret,
                        TextPatternRangeEndpoint_Start,
                    )
                    .ok()?;
                range
                    .MoveEndpointByUnit(
                        TextPatternRangeEndpoint_Start,
                        TextUnit_Character,
                        -span,
                    )
                    .ok()?;
                range.GetText(-1).ok()
            })
            .map(|s| s.to_string())
            .unwrap_or_default();

        // After: collapse to the trailing edge, then extend forwards.
        let after = caret
            .Clone()
            .ok()
            .and_then(|range| {
                range
                    .MoveEndpointByRange(
                        TextPatternRangeEndpoint_Start,
                        &caret,
                        TextPatternRangeEndpoint_End,
                    )
                    .ok()?;
                range
                    .MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, span)
                    .ok()?;
                range.GetText(-1).ok()
            })
            .map(|s| s.to_string())
            .unwrap_or_default();

        Some(CaretContext {
            before: cap_tail(&before),
            after: cap_head(&after),
        })
    }

    /// Keep the LAST `MAX_CARET_CHARS` characters — the end of `before` is the
    /// part adjacent to the caret, so that is the part that matters.
    fn cap_tail(s: &str) -> String {
        let count = s.chars().count();
        if count <= MAX_CARET_CHARS {
            return s.to_string();
        }
        s.chars().skip(count - MAX_CARET_CHARS).collect()
    }

    /// Keep the FIRST `MAX_CARET_CHARS` characters — the start of `after` is the
    /// part adjacent to the caret.
    fn cap_head(s: &str) -> String {
        s.chars().take(MAX_CARET_CHARS).collect()
    }

    /// What shape of field the caret is in.
    ///
    /// The single/multi-line split leans on a finding already recorded in
    /// [`read_text_content`]: multiline editors expose `TextPattern` but often
    /// NOT `ValuePattern`, while one-line inputs do the reverse. These are
    /// current-property calls rather than cached ones, but they are made against
    /// exactly ONE element, so the round-trips the cache request exists to
    /// eliminate are not in play here.
    unsafe fn field_kind(element: &IUIAutomationElement) -> FieldKind {
        if is_password(element) {
            return FieldKind::Password;
        }
        let control_type = element.CurrentControlType().ok();
        if control_type == Some(UIA_DocumentControlTypeId) {
            return FieldKind::MultiLine;
        }
        let has_text = element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .is_ok();
        if has_text {
            return FieldKind::MultiLine;
        }
        if control_type == Some(UIA_EditControlTypeId) {
            return FieldKind::SingleLine;
        }
        FieldKind::Unknown
    }

    /// Address-bar URL → host, **browser-agnostic**. Rather than assume the first
    /// `Edit` descendant is the address bar (true on Chromium, false on Gecko /
    /// heavily-customized chromes like Zen), enumerate every `Edit` descendant and
    /// return the first whose value parses as a host. Tree order puts the browser
    /// chrome before page content, so the real address bar wins over any page
    /// input that happens to hold a URL. If a browser's URL bar is collapsed out
    /// of the tree (e.g. Zen compact mode with the bar hidden), nothing is found
    /// and we degrade to the generic Browser category — no error, no UI.
    unsafe fn read_url(automation: &IUIAutomation, hwnd: HWND) -> Option<String> {
        descendants_of_type(
            automation,
            hwnd,
            UIA_EditControlTypeId.0,
            MAX_EDIT_SCAN,
        )
        .into_iter()
        // The URL can surface as the edit's value (typical) or, on some
        // browsers, its name — try both.
        .find_map(|edit| {
            read_value(&edit)
                .or_else(|| read_name(&edit))
                .and_then(|v| host_from_url(&v))
        })
    }

    // [GRAIN] `read_focused_terms` used to fetch the focused element a second
    // time to extract terms. The focus-anchored scan already holds that element,
    // so the term extraction is inline in `read` and this is gone rather than
    // kept as a second way to do the same thing.

    /// Harvest the foreground window's visible text from its accessibility tree.
    ///
    /// Collects `Text`, `Edit` and `Document` elements — static labels and prose
    /// come through as `Text`, editable and document content through the other
    /// two — and stops at [`MAX_WINDOW_TEXT_CHARS`].
    ///
    /// Two things it deliberately does NOT do. It never touches a password
    /// field, checked per element rather than assumed from the control type. And
    /// it de-duplicates, because accessibility trees repeat the same string at
    /// several levels (a button's label appears on the button, its text child,
    /// and often a wrapper) and a naive harvest is mostly the same words over and
    /// over — which wastes exactly the context budget this is spending.
    pub(in crate::context_detect) fn read_window_text() -> Option<String> {
        unsafe {
            let _com = ComGuard::init();
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let hwnd = super::windows_impl::foreground_window()?;

            let mut out = String::new();
            // Tracked incrementally rather than recomputed. `out.chars().count()`
            // is O(len), and checking it per element made the harvest quadratic
            // in the amount of text collected — on a busy page, the dominant cost.
            let mut collected = 0usize;
            let mut seen = std::collections::HashSet::new();

            'outer: for control_type in [
                UIA_TextControlTypeId.0,
                UIA_EditControlTypeId.0,
                UIA_DocumentControlTypeId.0,
            ] {
                for element in
                    descendants_of_type(&automation, hwnd, control_type, MAX_WINDOW_ELEMENT_SCAN)
                {
                    if collected >= super::MAX_WINDOW_TEXT_CHARS {
                        // Full: stop entirely rather than fall through to another
                        // whole-tree scan whose results cannot be used.
                        break 'outer;
                    }
                    if is_password(&element) {
                        continue;
                    }
                    // Static text carries its content in Name; editable and
                    // document elements carry it in Value.
                    let Some(text) = read_value(&element).or_else(|| read_name(&element)) else {
                        continue;
                    };
                    let text = text.trim();
                    let len = text.chars().count();
                    if len < 2 {
                        continue;
                    }
                    if !seen.insert(text.to_string()) {
                        continue;
                    }
                    out.push_str(text);
                    out.push('\n');
                    collected += len + 1;
                }
            }

            let out: String = out.chars().take(super::MAX_WINDOW_TEXT_CHARS).collect();
            let out = out.trim().to_string();
            (!out.is_empty()).then_some(out)
        }
    }

    /// The focused field's raw text. Own COM scope so it is safe to call
    /// standalone from any thread. Password fields skipped.
    pub(in crate::context_detect) fn read_focused_value() -> Option<String> {
        unsafe {
            let _com = ComGuard::init();
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let el = automation.GetFocusedElement().ok()?;
            if is_password(&el) {
                return None;
            }
            read_text_content(&el)
        }
    }

    /// The element's `ValuePattern` value as a non-empty `String`, if available.
    /// (Single-line edits, address bars, most web inputs.)
    unsafe fn read_value(el: &IUIAutomationElement) -> Option<String> {
        let vp: IUIAutomationValuePattern = el.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
        let bstr = vp.CurrentValue().ok()?;
        let s = bstr.to_string();
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    }

    /// Full textual content of an editable element: `ValuePattern` first (cheap,
    /// covers single-line inputs), falling back to `TextPattern`'s document range.
    /// **This is essential for multiline editors** (Notepad, VS Code, chat/mail
    /// composers, browser `<textarea>`s), which expose `TextPattern` but often NOT
    /// `ValuePattern` — reading only ValuePattern returned nothing there. Capped.
    unsafe fn read_text_content(el: &IUIAutomationElement) -> Option<String> {
        if let Some(v) = read_value(el) {
            return Some(cap(v));
        }
        if let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
            if let Ok(range) = tp.DocumentRange() {
                if let Ok(bstr) = range.GetText(MAX_TEXT_CHARS as i32) {
                    let s = bstr.to_string();
                    if !s.trim().is_empty() {
                        return Some(s);
                    }
                }
            }
        }
        None
    }

    fn cap(mut s: String) -> String {
        if s.len() > MAX_TEXT_CHARS {
            s.truncate(MAX_TEXT_CHARS);
        }
        s
    }

    /// The element's Name as a non-empty `String` (URL fallback on some browsers).
    unsafe fn read_name(el: &IUIAutomationElement) -> Option<String> {
        let s = el.CurrentName().ok()?.to_string();
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    }

    unsafe fn is_password(el: &IUIAutomationElement) -> bool {
        el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false)
    }

    /// Control types that unambiguously accept no typed text.
    ///
    /// Deliberately excludes the CONTAINER types — `Pane`, `Window`, `Group`,
    /// `Custom`, `List`, `Tree`, `ToolBar`. Electron apps, canvas editors and
    /// custom chromes routinely report those for surfaces that are perfectly
    /// editable, and a wrong `NotEditable` is the expensive error here (it
    /// suppresses a paste the user wanted). Leaf widgets are safe; containers
    /// are not.
    fn is_non_text_control(control_type: i32) -> bool {
        [
            UIA_ButtonControlTypeId,
            UIA_CalendarControlTypeId,
            UIA_CheckBoxControlTypeId,
            UIA_HeaderControlTypeId,
            UIA_HeaderItemControlTypeId,
            UIA_HyperlinkControlTypeId,
            UIA_ImageControlTypeId,
            UIA_ListItemControlTypeId,
            UIA_MenuBarControlTypeId,
            UIA_MenuItemControlTypeId,
            UIA_ProgressBarControlTypeId,
            UIA_RadioButtonControlTypeId,
            UIA_ScrollBarControlTypeId,
            UIA_SeparatorControlTypeId,
            UIA_SliderControlTypeId,
            UIA_SplitButtonControlTypeId,
            UIA_StatusBarControlTypeId,
            UIA_TabItemControlTypeId,
            UIA_TextControlTypeId,
            UIA_ThumbControlTypeId,
            UIA_TitleBarControlTypeId,
            UIA_TreeItemControlTypeId,
        ]
        .iter()
        .any(|id| id.0 == control_type)
    }

    /// Read the focused element into [`FocusFacts`]. Own COM scope, so it is
    /// safe to call standalone from the paste thread.
    ///
    /// Only cheap calls: control type, pattern availability, and one selection
    /// probe. No text is read — this runs on the paste path, where the user is
    /// already waiting, and reading a large document's text there would be the
    /// one thing that makes dictation feel slow.
    pub(in crate::context_detect) fn read_focus_facts() -> FocusFacts {
        unsafe {
            let mut facts = FocusFacts::default();
            let _com = ComGuard::init();
            let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            else {
                return facts;
            };
            let Ok(el) = automation.GetFocusedElement() else {
                return facts;
            };

            facts.is_password = is_password(&el);
            if facts.is_password {
                // Nothing else is read off a password field, ever.
                return facts;
            }

            facts.has_text_edit_pattern = el
                .GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId)
                .is_ok();

            facts.value_read_only = el
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|vp| vp.CurrentIsReadOnly().ok())
                .map(|b| b.as_bool());

            if let Ok(control_type) = el.CurrentControlType() {
                facts.control = if control_type == UIA_EditControlTypeId {
                    ControlClass::Edit
                } else if control_type == UIA_DocumentControlTypeId {
                    ControlClass::Document
                } else if is_non_text_control(control_type.0) {
                    ControlClass::NonText
                } else {
                    ControlClass::Other
                };
            }

            // Only asked when it can change the verdict (see `classify`): a
            // Document with a caret is ambiguous, without one it is read-only.
            if facts.control == ControlClass::Document {
                facts.has_caret = el
                    .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                    .ok()
                    .and_then(|tp| tp.GetSelection().ok())
                    .and_then(|sel| sel.GetElement(0).ok())
                    .is_some();
            }

            facts
        }
    }
}

/// Parse a hostname out of a browser address-bar string. Returns `None` for
/// anything that isn't host-shaped (e.g. a search query with spaces, or an empty
/// bar). Strips scheme, userinfo, path, port, and a leading `www.`; lowercases.
#[cfg(windows)]
fn host_from_url(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return None; // a search query, not a URL.
    }
    // Drop scheme.
    let s = s.split("://").last().unwrap_or(s);
    // Host is up to the first '/', '?', or '#'.
    let host = s
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(s)
        .rsplit('@') // strip any userinfo
        .next()
        .unwrap_or(s);
    // Strip port.
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    // Must look like a domain: at least one dot and only host-legal chars.
    if host.contains('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        Some(host)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_core::AppSettings;

    fn ctx(exe: &str, category: AppCategory) -> ActiveContext {
        ActiveContext {
            app_name: exe.to_string(),
            exe: exe.to_string(),
            exe_path: String::new(),
            category,
            url_host: None,
            field: FieldKind::Unknown,
            region: None,
            confidence: Confidence::Guess,
            caret: None,
            nearby_terms: Vec::new(),
        }
    }

    #[test]
    fn category_mapping_covers_common_apps() {
        assert_eq!(category_for_exe("code"), AppCategory::Ide);
        assert_eq!(category_for_exe("cursor"), AppCategory::Ide);
        assert_eq!(category_for_exe("outlook"), AppCategory::Email);
        assert_eq!(category_for_exe("slack"), AppCategory::WorkChat);
        assert_eq!(category_for_exe("whatsapp"), AppCategory::PersonalChat);
        assert_eq!(category_for_exe("notion"), AppCategory::Docs);
        assert_eq!(category_for_exe("chrome"), AppCategory::Browser);
        assert_eq!(category_for_exe("some_unknown_app"), AppCategory::Other);
    }

    /// Terminals are no longer IDEs: a shell wants a command, an editor wants
    /// code, and the two soft lines say different things.
    #[test]
    fn terminals_split_from_ides() {
        assert_eq!(category_for_exe("pwsh"), AppCategory::Terminal);
        assert_eq!(category_for_exe("wt"), AppCategory::Terminal);
        assert_eq!(category_for_exe("alacritty"), AppCategory::Terminal);
        assert_eq!(category_for_exe("ghostty"), AppCategory::Terminal);
        // …and the editors stayed put.
        assert_eq!(category_for_exe("code"), AppCategory::Ide);
        assert_eq!(category_for_exe("nvim"), AppCategory::Ide);
    }

    /// The hole this phase exists to close: a browser tab now resolves to what
    /// the SITE is, not to "a browser".
    #[test]
    fn site_table_resolves_webapps_to_real_categories() {
        assert_eq!(category_for_site("mail.google.com"), Some(AppCategory::Email));
        assert_eq!(category_for_site("claude.ai"), Some(AppCategory::AiChat));
        assert_eq!(category_for_site("github.com"), Some(AppCategory::CodeReview));
        assert_eq!(category_for_site("linear.app"), Some(AppCategory::Ticket));
        assert_eq!(category_for_site("app.slack.com"), Some(AppCategory::WorkChat));
        assert_eq!(category_for_site("docs.google.com"), Some(AppCategory::Docs));
        assert_eq!(category_for_site("x.com"), Some(AppCategory::Social));
        // Unknown sites resolve to nothing, so the caller keeps `Browser`.
        assert_eq!(category_for_site("example.com"), None);
    }

    /// Subdomains inherit, and `www.` is irrelevant.
    #[test]
    fn site_matching_accepts_subdomains_and_strips_www() {
        assert_eq!(category_for_site("www.github.com"), Some(AppCategory::CodeReview));
        assert_eq!(category_for_site("gist.github.com"), Some(AppCategory::CodeReview));
        assert_eq!(category_for_site("acme.atlassian.net"), Some(AppCategory::Ticket));
    }

    /// The dot boundary is a security property, not a nicety: a bare
    /// `ends_with` would hand any attacker-registered lookalike the tone (and
    /// later, the per-site rules) of the domain it impersonates.
    #[test]
    fn site_matching_refuses_lookalike_domains() {
        assert_eq!(category_for_site("notgithub.com"), None);
        assert_eq!(category_for_site("evil-slack.com"), None);
        assert_eq!(category_for_site("fakex.com"), None);
        assert_eq!(category_for_site("mail.google.com.evil.tld"), None);
    }

    /// Trailing-dot patterns are prefix wildcards, for self-hosted installs
    /// that name themselves by product (`jira.acme.com`).
    #[test]
    fn site_matching_supports_selfhosted_prefixes() {
        assert_eq!(category_for_site("jira.acme.com"), Some(AppCategory::Ticket));
        assert_eq!(category_for_site("confluence.acme.com"), Some(AppCategory::Docs));
        // A prefix wildcard must not match a longer first label.
        assert_eq!(category_for_site("myjira.acme.com"), None);
    }

    /// Ordering contract: specific rows are matched before the general ones
    /// they are subdomains of. If the table is ever reordered so a general row
    /// shadows a specific one, this catches it.
    #[test]
    fn specific_sites_win_over_general_ones() {
        // chat.google.com is WorkChat even though docs.google.com is Docs —
        // neither may collapse into the other.
        assert_eq!(category_for_site("chat.google.com"), Some(AppCategory::WorkChat));
        assert_eq!(category_for_site("docs.google.com"), Some(AppCategory::Docs));
        assert_eq!(category_for_site("gemini.google.com"), Some(AppCategory::AiChat));
    }

    /// A single-line field must not get a trailing period bolted on. This is the
    /// daily papercut the field detection exists for.
    #[test]
    fn single_line_field_suppresses_terminal_punctuation() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("chrome", AppCategory::Browser);
        c.field = FieldKind::SingleLine;
        let out = compose_prompt("BASE ${output}", &s, Some(&c), None);
        assert!(out.contains("SINGLE-LINE"));
        assert!(out.contains("do not add a trailing period"));
    }

    /// A multi-line editor must NOT get the one-line clause — that would stop
    /// ordinary prose from being punctuated at all.
    #[test]
    fn multiline_field_keeps_normal_punctuation() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("winword", AppCategory::Docs);
        c.field = FieldKind::MultiLine;
        let out = compose_prompt("BASE ${output}", &s, Some(&c), None);
        assert!(!out.contains("SINGLE-LINE"));
    }

    /// The seam contract must both supply the surroundings AND forbid the model
    /// from treating them as input — a model handed context will otherwise
    /// continue the sentence it can see instead of formatting the one it was given.
    #[test]
    fn caret_context_supplies_the_seam_and_forbids_echoing_it() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("winword", AppCategory::Docs);
        c.caret = Some(CaretContext {
            before: "We agreed the release ".into(),
            after: " before the holidays.".into(),
        });
        let out = compose_prompt("BASE ${output}", &s, Some(&c), None);

        assert!(out.contains("We agreed the release"));
        assert!(out.contains("before the holidays."));
        // The prohibition must bracket the excerpts — before AND after — and the
        // whole prompt must end with the output constraint.
        assert!(out.contains("NOT input, NOT \nto be repeated") || out.contains("NOT input"));
        assert!(out.contains("Never output any part of them"));
        assert!(out.trim_end().ends_with("no notes, no explanation."));
        // The user's own prompt still survives verbatim inside.
        assert!(out.contains("BASE ${output}"));
    }

    /// Excerpts must NOT be wrapped in XML-ish tags. Tags around prose read as
    /// "content to emit" and were echoed into a user's email draft verbatim.
    #[test]
    fn caret_excerpts_carry_no_xml_tags() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("winword", AppCategory::Docs);
        c.caret = Some(CaretContext {
            before: "Dear Rita,".into(),
            after: "Regards".into(),
        });
        let out = compose_prompt("BASE", &s, Some(&c), None);
        assert!(!out.contains("<before_text>"));
        assert!(!out.contains("<after_text>"));
    }

    /// One side of the seam may be empty (dictating at the start or end of a
    /// field); that side must not be mentioned at all.
    #[test]
    fn empty_caret_side_is_omitted() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("winword", AppCategory::Docs);
        c.caret = Some(CaretContext {
            before: "Dear Rita,".into(),
            after: String::new(),
        });
        let out = compose_prompt("BASE", &s, Some(&c), None);
        assert!(out.contains("immediately before:"));
        assert!(!out.contains("immediately after:"));
    }

    /// Any applied context layer must be followed by the terminal output
    /// constraint — the one instruction that sits in the most-attended position.
    #[test]
    fn context_always_ends_with_the_output_constraint() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let out = compose_prompt("BASE", &s, Some(&ctx("code", AppCategory::Ide)), None);
        assert!(out.trim_end().ends_with("no notes, no explanation."));

        // …and a prompt with NO context added stays byte-for-byte the base.
        let bare = ctx("unknownapp", AppCategory::Other);
        assert_eq!(compose_prompt("BASE", &s, Some(&bare), None), "BASE");
    }

    /// Off means off: with the opt-in disabled nothing is captured, so no caret
    /// reaches the prompt and the base is untouched.
    #[test]
    fn no_caret_context_leaves_the_prompt_alone() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let c = ctx("unknownapp", AppCategory::Other); // no soft line, no terms
        assert_eq!(compose_prompt("BASE", &s, Some(&c), None), "BASE");
    }

    /// The spoken instruction still outranks the seam, same as every other layer.
    #[test]
    fn spoken_instruction_still_outranks_caret_context() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("winword", AppCategory::Docs);
        c.caret = Some(CaretContext {
            before: "before".into(),
            after: "after".into(),
        });
        let out = compose_prompt("BASE", &s, Some(&c), Some("make it a haiku"));
        let spoken = out.find("make it a haiku").unwrap();
        let seam = out.find("immediately before:").unwrap();
        assert!(spoken < seam, "spoken instruction must precede the seam");
    }

    /// A site may only override the app category on structural evidence. This is
    /// the "a wrong rule is worse than no rule" gate.
    #[test]
    fn probable_confidence_does_not_assert_the_site() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;

        let mut exact = ctx("chrome", AppCategory::Email);
        exact.url_host = Some("mail.google.com".into());
        exact.confidence = Confidence::Exact;
        let out = compose_prompt("BASE", &s, Some(&exact), None);
        assert!(out.contains("website: mail.google.com"));

        // Same host, weaker evidence: it may have come from another tab, so the
        // prompt must not claim it.
        let mut probable = exact.clone();
        probable.confidence = Confidence::Probable;
        let out = compose_prompt("BASE", &s, Some(&probable), None);
        assert!(!out.contains("website:"));
    }

    #[test]
    fn only_exact_confidence_may_override_the_app_category() {
        assert!(Confidence::Exact.allows_site_category());
        assert!(!Confidence::Probable.allows_site_category());
        assert!(!Confidence::Guess.allows_site_category());
    }

    /// "main" is the default region of every page and says nothing worth tokens;
    /// a sidebar or dialog genuinely disambiguates.
    #[test]
    fn only_disambiguating_regions_reach_the_prompt() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;

        let mut main = ctx("chrome", AppCategory::Docs);
        main.region = Some("main".into());
        assert!(!compose_prompt("BASE", &s, Some(&main), None).contains("page region"));

        let mut side = ctx("chrome", AppCategory::Docs);
        side.region = Some("complementary".into());
        assert!(compose_prompt("BASE", &s, Some(&side), None).contains("page region: complementary"));
    }

    /// The AI-prompt line has to say "do less" — smoothing a prompt's wording
    /// away is the failure mode there, so guard the wording that prevents it.
    #[test]
    fn ai_chat_soft_line_forbids_polishing() {
        let line = AppCategory::AiChat.soft_line().unwrap();
        assert!(line.contains("instruction"));
        assert!(line.contains("never soften"));
    }

    /// Every category that adds context must keep at least one negative guard;
    /// that is what makes this layer SOFT rather than hard formatting.
    #[test]
    fn soft_lines_stay_soft_and_bounded() {
        for category in [
            AppCategory::Ide,
            AppCategory::Terminal,
            AppCategory::Email,
            AppCategory::WorkChat,
            AppCategory::PersonalChat,
            AppCategory::Social,
            AppCategory::Docs,
            AppCategory::CodeReview,
            AppCategory::AiChat,
            AppCategory::Ticket,
            AppCategory::Browser,
        ] {
            let line = category.soft_line().expect("category adds context");
            let lower = line.to_ascii_lowercase();
            assert!(
                ["no ", "not ", "never", "unless dictated"]
                    .iter()
                    .any(|guard| lower.contains(guard)),
                "{category:?} lost its negative guard: {line}"
            );
            // Rides on every dictation — keep it cheap (~4 bytes/token).
            assert!(
                line.len() <= 260,
                "{category:?} soft line is {} bytes, too costly per utterance",
                line.len()
            );
        }
        assert!(AppCategory::Other.soft_line().is_none());
    }

    #[test]
    fn disabled_returns_base_untouched() {
        let s = AppSettings::default(); // context_awareness_enabled = false
        let base = "BASE PROMPT ${output}";
        assert_eq!(
            compose_prompt(base, &s, Some(&ctx("code", AppCategory::Ide)), None),
            base
        );
    }

    #[test]
    fn other_category_with_no_mode_adds_nothing() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let base = "BASE ${output}";
        assert_eq!(
            compose_prompt(base, &s, Some(&ctx("unknownapp", AppCategory::Other)), None),
            base
        );
    }

    #[test]
    fn soft_context_is_prepended_for_known_category() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let base = "BASE ${output}";
        let out = compose_prompt(base, &s, Some(&ctx("code", AppCategory::Ide)), None);
        assert!(out.starts_with("[Context awareness]"));
        assert!(out.contains("code editor"));
        // The base survives verbatim; the terminal output constraint follows it
        // (see `context_always_ends_with_the_output_constraint`).
        assert!(out.contains(base));
    }

    #[test]
    fn extract_terms_keeps_names_and_identifiers_drops_prose() {
        let text = "I asked Rita to fix the useGrainStore hook and the snake_case bug in PyTorch v2 today because it was broken";
        let terms = extract_unique_terms(text);
        assert!(terms.contains(&"Rita".to_string()));
        assert!(terms.contains(&"useGrainStore".to_string()));
        assert!(terms.contains(&"snake_case".to_string()));
        assert!(terms.contains(&"PyTorch".to_string()));
        // Ordinary lowercase prose contributes nothing.
        for w in [
            "asked", "the", "hook", "bug", "today", "because", "was", "broken",
        ] {
            assert!(!terms.iter().any(|t| t == w), "leaked prose word: {w}");
        }
    }

    #[test]
    fn extract_terms_dedups_and_caps() {
        let text = "Rita Rita Rita ".repeat(20);
        let terms = extract_unique_terms(&text);
        assert_eq!(terms, vec!["Rita".to_string()]); // de-duped.
        let many: String = (0..50).map(|i| format!("Ident{i} ")).collect();
        assert!(extract_unique_terms(&many).len() <= MAX_NEARBY_TERMS);
    }

    #[test]
    fn nearby_terms_hint_added_even_without_soft_or_mode() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("unknownapp", AppCategory::Other);
        c.nearby_terms = vec!["Rita".into(), "PyTorch".into()];
        let out = compose_prompt("BASE ${output}", &s, Some(&c), None);
        assert!(out.contains("Nearby terms"));
        assert!(out.contains("Rita, PyTorch"));
    }

    #[test]
    fn spoken_instruction_added_even_with_context_off() {
        let s = AppSettings::default(); // context_awareness_enabled = false
        let base = "BASE ${output}";
        let out = compose_prompt(base, &s, None, Some("  make it a haiku  "));
        assert!(out.starts_with("[Spoken instruction"));
        assert!(out.contains("HIGHEST PRIORITY"));
        assert!(out.contains("make it a haiku")); // trimmed, present
        assert!(out.ends_with(base)); // base preserved verbatim at the tail.
    }

    #[test]
    fn blank_spoken_instruction_is_ignored() {
        let s = AppSettings::default();
        let base = "BASE";
        assert_eq!(compose_prompt(base, &s, None, Some("   ")), base);
        assert_eq!(compose_prompt(base, &s, None, None), base);
    }

    #[test]
    fn spoken_instruction_outranks_soft_context() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let out = compose_prompt(
            "BASE ${output}",
            &s,
            Some(&ctx("code", AppCategory::Ide)),
            Some("translate it into French"),
        );
        // Order carries the authority: what the user just dictated for THIS
        // transcript has to be read before the automatic context nudge.
        let spoken_pos = out.find("translate it into French").unwrap();
        let ctx_pos = out.find("[Context awareness]").unwrap();
        assert!(
            spoken_pos < ctx_pos,
            "spoken instruction must precede soft context"
        );
    }

    #[cfg(windows)]
    #[test]
    fn host_from_url_parsing() {
        assert_eq!(
            host_from_url("https://mail.google.com/mail/u/0"),
            Some("mail.google.com".into())
        );
        assert_eq!(
            host_from_url("mail.google.com/mail"),
            Some("mail.google.com".into())
        );
        assert_eq!(
            host_from_url("https://www.x.com/home"),
            Some("x.com".into())
        );
        assert_eq!(
            host_from_url("user:pass@host.example.com:8080/p"),
            Some("host.example.com".into())
        );
        assert_eq!(host_from_url("how to parse a url"), None); // search query.
        assert_eq!(host_from_url(""), None);
        assert_eq!(host_from_url("localhost"), None); // no dot → not host-shaped.
    }
}
