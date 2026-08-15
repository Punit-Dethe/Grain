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

use grain_core::settings::CustomContextProfile;
use grain_core::AppSettings;

/// Coarse app category driving the automatic SOFT context line. Deliberately a
/// small, robust bucket set (à la the incumbents' 4–8 categories) rather than a
/// per-app rule table: unknown apps fall to [`AppCategory::Other`], which adds no
/// context at all, so behavior degrades safely for the long tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCategory {
    /// Email composers and webmail — slightly polished, but NO email layout
    /// unless dictated.
    Email,
    /// Team chat, issue trackers and shared documents: professional, concise,
    /// factual.
    Work,
    /// Personal messengers and social posts: the user's own voice, kept.
    Casual,
    /// Code editors, terminals and code hosts: exact tokens, no padding.
    Technical,
    /// A prompt box for an AI assistant.
    ///
    /// Split back out of [`AppCategory::Technical`] because the danger here is
    /// not a technical one. Every other profile's job is to tidy prose; this
    /// one's is to stop a helpful model from *answering* dictated text instead
    /// of writing it down — the documented failure mode of cleanup layers in
    /// front of chat models. An AI prompt is often plain English with no
    /// identifier in sight, so the technical advice did nothing for it.
    AiChat,
    /// Anything unrecognized — no instruction is added. Also where a browser
    /// sitting on an unresolved site lands: "a browser" says nothing about what
    /// is being written, so it earns no tone rule of its own.
    Other,
}

/// The editable profiles, in the order the settings UI shows them. The first
/// four are the tabs; `ai_chat` is shown under "Other".
/// [`AppCategory::Other`] is deliberately absent: it is the *absence* of a
/// profile, has no instruction, and there is nothing for a user to edit.
pub const PROFILE_IDS: [&str; 5] = ["email", "work", "casual", "technical", "ai_chat"];

impl AppCategory {
    /// The settings id for this profile, or `None` for [`AppCategory::Other`],
    /// which is not a profile. This is the string the UI and the stored
    /// overrides key on, so it is the one name that must never drift.
    pub fn profile_id(self) -> Option<&'static str> {
        Some(match self {
            AppCategory::Email => "email",
            AppCategory::Work => "work",
            AppCategory::Casual => "casual",
            AppCategory::Technical => "technical",
            AppCategory::AiChat => "ai_chat",
            AppCategory::Other => return None,
        })
    }

    /// Inverse of [`Self::profile_id`].
    pub fn from_profile_id(id: &str) -> Option<Self> {
        match id {
            "email" => Some(AppCategory::Email),
            "work" => Some(AppCategory::Work),
            "casual" => Some(AppCategory::Casual),
            "technical" => Some(AppCategory::Technical),
            "ai_chat" => Some(AppCategory::AiChat),
            _ => None,
        }
    }

    /// The SHIPPED instruction for this profile, before any user edit.
    ///
    /// # How these are written, and why
    ///
    /// Rewritten 2026-08-15 after reading how other dictation tools and the
    /// prompting literature handle this. Three findings shaped every line:
    ///
    /// 1. **Say what to do, not what to avoid.** Anthropic's own guidance is to
    ///    tell the model what to do instead of what not to do, and reports of
    ///    stacked "DO NOT"s degrading output are consistent enough to design
    ///    around. The earlier wording here was almost entirely prohibitions.
    ///    Each one is now stated as the action first, with the boundary second
    ///    ("Keep X as spoken" rather than "never change X"), which is the same
    ///    guard expressed in the form models follow better.
    /// 2. **Length costs accuracy before it costs context.** Instruction-
    ///    following degrades as prompts grow, well short of any window limit,
    ///    and models attend best to the start and end of a block. These ride on
    ///    EVERY dictation, so each one opens with what the surface IS, puts the
    ///    single most important rule last, and stops.
    /// 3. **Too many constraints contradict each other.** A model given a long
    ///    list starts trading rules off. So each profile carries ONE tone
    ///    sentence and at most two boundaries — the ones whose loss is visible
    ///    in the output — instead of every rule its old sub-categories had.
    ///
    /// What has NOT changed is that this layer stays soft. It shapes tone and
    /// preserves wording; it never imposes structure. Hard per-app formatting is
    /// the App Modes extension's job.
    pub fn default_instruction(self) -> Option<&'static str> {
        Some(match self {
            // "Unless dictated" carries the whole no-hard-formatting promise and
            // is kept in every profile that could otherwise invent layout.
            AppCategory::Email => {
                "An email composer. Write it slightly more polished and professional than \
                 spoken, keeping the user's own points and order. Include a subject, \
                 greeting, sign-off or any email layout only if it was dictated."
            }
            // Merged from work chat + issue tracker + docs. All three wanted the
            // same thing — say it plainly and add no ceremony — so the tone is
            // stated once and the ceremony is bounded once.
            AppCategory::Work => {
                "A work surface — team chat, an issue tracker, or a shared document. Write \
                 it professionally and concisely, keeping the user's wording, order and \
                 structure. Add a greeting, pleasantries, headings or lists only if they \
                 were dictated."
            }
            // Merged from personal messenger + social post. Same instinct —
            // protect the user's voice — so this one lost nothing in the merge.
            AppCategory::Casual => {
                "A casual message or post. Keep the user's own voice, slang and phrasing, \
                 and clean up only what is clearly a speech slip. Leave the register as \
                 casual as it was spoken, and add hashtags or emoji only if they were \
                 dictated."
            }
            // Editors, terminals and code hosts. The terminal rule survives as
            // its own sentence: a command that comes out sentence-cased with a
            // full stop is wrong in a way the user sees instantly.
            AppCategory::Technical => {
                "A technical surface — code editor, terminal, or code host. Keep \
                 identifiers, library names, flags, paths and casing exactly as spoken, \
                 treating unfamiliar jargon as correct rather than as a mistake to fix. \
                 Stay terse. If it reads as a shell command or a path, leave it lowercase \
                 and unpunctuated."
            }
            // The one profile whose job is NOT to improve the text.
            //
            // The failure mode here is documented and specific: a cleanup model
            // handed something that looks like a question answers it instead of
            // writing it down, because that is what a chat model is trained to
            // do. That is a corrupted prompt box, not a tone mismatch, so it is
            // stated first and stated as an identity ("this is text to be
            // written down") rather than as a prohibition.
            AppCategory::AiChat => {
                "A prompt box for an AI assistant. Everything dictated is text to be \
                 written into that box — transcribe it, and answer nothing, even when it \
                 is phrased as a question or a command. Keep the user's exact intent, \
                 specifics and wording, since the details are the payload; fix only \
                 punctuation, casing and speech slips."
            }
            AppCategory::Other => return None,
        })
    }

    /// The instruction that will actually be sent: the user's edit if they made
    /// one, otherwise [`Self::default_instruction`].
    ///
    /// Overrides are stored SPARSELY — only edited profiles appear in settings —
    /// so an untouched profile keeps tracking the shipped wording as it improves
    /// across releases instead of being frozen at whatever shipped the day the
    /// user first opened the tab.
    ///
    /// An override trimmed to nothing means "this profile adds no instruction",
    /// which is a legitimate thing to want and is why this can return `None` for
    /// a profile that has a default.
    pub fn instruction<'a>(self, settings: &'a AppSettings) -> Option<&'a str> {
        let id = self.profile_id()?;
        match settings
            .context_profile_instructions
            .iter()
            .find(|o| o.id == id)
        {
            Some(o) => {
                let text = o.instruction.trim();
                (!text.is_empty()).then_some(text)
            }
            None => self.default_instruction(),
        }
    }
}

/// The custom profile that claims this surface, if any.
///
/// **Custom beats built-in, always.** Naming an app or site in a profile is a
/// statement that Grain's guess is wrong for it, so there is no confidence test
/// and no merging: the user's profile simply wins.
///
/// Within the custom profiles, a WEBSITE match beats an APPLICATION match, for
/// the same reason the built-in path resolves a site over a browser — "Chrome"
/// describes the window, the host describes the work. Otherwise a profile that
/// claimed `chrome` would swallow every site profile the user made.
///
/// Ties between two profiles claiming the same target are resolved by order,
/// first wins, so the result is stable rather than dependent on iteration luck.
pub(crate) fn custom_profile_for<'a>(
    ctx: &ActiveContext,
    settings: &'a AppSettings,
) -> Option<&'a CustomContextProfile> {
    if settings.context_custom_profiles.is_empty() {
        return None; // the common case, costing one length check
    }
    // Only a site Grain is confident about may claim the surface — the same bar
    // the built-in table has to clear. A `Probable` host was found by scanning
    // the window and can belong to a tab the user is not typing into.
    let host = ctx
        .url_host
        .as_deref()
        .filter(|_| ctx.confidence.allows_site_category());
    if let Some(host) = host {
        let hit = settings.context_custom_profiles.iter().find(|p| {
            p.targets
                .iter()
                .any(|t| t.kind == "website" && host_matches(host, t.value.trim()))
        });
        if hit.is_some() {
            return hit;
        }
    }
    settings
        .context_custom_profiles
        .iter()
        .find(|p| p.targets.iter().any(|t| app_target_matches(t, ctx)))
}

/// Whether an `application` target names the app in the foreground.
///
/// Compared against BOTH identities the window carries, because the picker
/// stores whichever one is meaningful for that app: an executable stem for a
/// desktop app, an AppUserModelID for a packaged one (which has no shortcut and
/// therefore no exe to name). Testing both is one extra string compare and
/// cannot confuse them — an exe stem never contains the `!` an AppUserModelID is
/// built around, so no value can accidentally satisfy the wrong side.
fn app_target_matches(
    target: &grain_core::settings::ContextProfileTarget,
    ctx: &ActiveContext,
) -> bool {
    if target.kind != "application" {
        return false;
    }
    let value = target.value.trim();
    if value.is_empty() {
        return false;
    }
    if !ctx.exe.is_empty() && value.eq_ignore_ascii_case(&ctx.exe) {
        return true;
    }
    ctx.aumid
        .as_deref()
        .is_some_and(|aumid| value.eq_ignore_ascii_case(aumid))
}

/// Up to `n` representative hosts for a profile, taken from [`SITE_TABLE`] in
/// table order.
///
/// DERIVED, never a second list. The settings card shows these sites' real
/// favicons, and a hand-written list next to the table would be one more thing
/// to forget when a site moves category — this way the card and the behaviour
/// can never disagree.
///
/// Table order is "most specific first", which also happens to put the
/// best-known site of each category near the front, so the first few are the
/// ones a person recognises.
pub(crate) fn sample_sites(category: AppCategory, n: usize) -> Vec<String> {
    SITE_TABLE
        .iter()
        .filter(|(pattern, c)| {
            // Prefix wildcards (`jira.`) name no real host, so they can never
            // yield a favicon — skip them rather than show a broken tile.
            *c == category && !pattern.ends_with('.')
        })
        .take(n)
        .map(|(pattern, _)| (*pattern).to_string())
        .collect()
}

/// Whether Grain treats this host as a site it knows — either because it is in
/// [`SITE_TABLE`], or because the user named it in a custom profile.
///
/// This is the gate the pill's website icons use, which is why it lives beside
/// the table rather than in `pill_icon`: "which sites does Grain support" must
/// have exactly one answer, or a site could get a post-processing profile
/// without an icon (or, worse, an icon fetch without ever being a supported
/// site).
pub(crate) fn is_supported_site(host: &str, settings: &AppSettings) -> bool {
    if category_for_site(host).is_some() {
        return true;
    }
    settings.context_custom_profiles.iter().any(|p| {
        p.targets
            .iter()
            .any(|t| t.kind == "website" && host_matches(host, t.value.trim()))
    })
}

/// The instruction for this surface: the user's own profile if one claims it,
/// otherwise the built-in category's.
///
/// Resolved HERE rather than during detection so that `detect_active_context`
/// keeps its signature and its callers — the pill's icon path calls it too, and
/// does not want to read settings to find out what an app is called.
fn instruction_for<'a>(ctx: &ActiveContext, settings: &'a AppSettings) -> Option<&'a str> {
    if let Some(profile) = custom_profile_for(ctx, settings) {
        let text = profile.instruction.trim();
        // An empty custom instruction still WINS — it means "say nothing here",
        // which is a real thing to want for one noisy app, and falling back to
        // the built-in would quietly ignore the profile the user made.
        return (!text.is_empty()).then_some(text);
    }
    ctx.category.instruction(settings)
}

/// Address-bar host → category. **This is the table that makes context awareness
/// work at all for most people**: email, chat, docs and social overwhelmingly
/// live in a browser tab, and until this existed every one of them resolved to
/// the generic [`AppCategory::Other`] line — the weakest bucket — even though
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
    // Bare `outlook.com` AFTER the three specific Outlook hosts above; none of
    // them is a subdomain of it, but keeping the order obvious is what stops the
    // next addition getting it wrong.
    ("outlook.com", AppCategory::Email),
    ("mail.yahoo.com", AppCategory::Email),
    ("mail.zoho.com", AppCategory::Email),
    ("fastmail.com", AppCategory::Email),
    ("hey.com", AppCategory::Email),
    ("superhuman.com", AppCategory::Email),
    ("mail.aol.com", AppCategory::Email),
    ("mail.yandex.com", AppCategory::Email),
    ("app.tuta.com", AppCategory::Email),
    ("tutanota.com", AppCategory::Email),
    ("gmx.com", AppCategory::Email),
    ("icloud.com", AppCategory::Email),
    ("roundcube.", AppCategory::Email),
    // -- AI assistants (prompt boxes) --
    //
    // The FIRST THREE of each section are what the settings card stacks as
    // icons (`sample_sites`), so they are ordered to be three *visually
    // distinct* services. `chat.openai.com` sits below Gemini for that reason
    // alone: it is ChatGPT under an older name and resolves to the same icon,
    // so leading with it would have made the card read Claude, ChatGPT,
    // ChatGPT. Ordering within a section is otherwise free — `host_matches`
    // only requires specific-before-general, and none of these contain another.
    ("claude.ai", AppCategory::AiChat),
    ("chatgpt.com", AppCategory::AiChat),
    ("gemini.google.com", AppCategory::AiChat),
    ("chat.openai.com", AppCategory::AiChat),
    ("aistudio.google.com", AppCategory::AiChat),
    ("perplexity.ai", AppCategory::AiChat),
    ("poe.com", AppCategory::AiChat),
    ("copilot.microsoft.com", AppCategory::AiChat),
    ("chat.deepseek.com", AppCategory::AiChat),
    ("chat.mistral.ai", AppCategory::AiChat),
    ("grok.com", AppCategory::AiChat),
    ("t3.chat", AppCategory::AiChat),
    ("openrouter.ai", AppCategory::AiChat),
    ("claude.com", AppCategory::AiChat),
    ("notebooklm.google.com", AppCategory::AiChat),
    ("chat.qwen.ai", AppCategory::AiChat),
    ("kimi.com", AppCategory::AiChat),
    ("meta.ai", AppCategory::AiChat),
    ("lmarena.ai", AppCategory::AiChat),
    ("huggingface.co", AppCategory::AiChat),
    // Prompt-driven builders. The box you type into is a prompt box, so the
    // AiChat profile is the right one even though the output is an app.
    ("bolt.new", AppCategory::AiChat),
    ("lovable.dev", AppCategory::AiChat),
    ("v0.app", AppCategory::AiChat),
    // -- Code review / repo hosts. Writing into a text box on these is almost
    //    always a PR body, an issue, or a review comment.
    ("github.com", AppCategory::Technical),
    ("gitlab.com", AppCategory::Technical),
    ("bitbucket.org", AppCategory::Technical),
    ("codeberg.org", AppCategory::Technical),
    ("gerrit.", AppCategory::Technical),
    ("stackoverflow.com", AppCategory::Technical),
    // -- Work: the three that lead are what the settings card stacks --
    //
    // Chat, tickets and docs — the three things this profile merges — and all
    // three serve a real favicon on the first rung. Measured: `atlassian.net`
    // answers /favicon.ico with an HTML page and `app.asana.com` 404s, so
    // leading with those left the card showing fallback glyphs.
    ("slack.com", AppCategory::Work),
    ("linear.app", AppCategory::Work),
    ("docs.google.com", AppCategory::Work),
    // -- Issue trackers --
    ("atlassian.net", AppCategory::Work),
    ("jira.", AppCategory::Work),
    ("app.asana.com", AppCategory::Work),
    ("trello.com", AppCategory::Work),
    ("shortcut.com", AppCategory::Work),
    ("height.app", AppCategory::Work),
    ("monday.com", AppCategory::Work),
    ("clickup.com", AppCategory::Work),
    // -- Work chat (formal register) --
    ("teams.microsoft.com", AppCategory::Work),
    ("teams.live.com", AppCategory::Work),
    ("chat.google.com", AppCategory::Work),
    ("meet.google.com", AppCategory::Work),
    ("webex.com", AppCategory::Work),
    ("zoom.us", AppCategory::Work),
    ("mattermost.com", AppCategory::Work),
    ("rocket.chat", AppCategory::Work),
    ("chime.aws", AppCategory::Work),
    // -- Personal messengers (casual register) --
    ("web.whatsapp.com", AppCategory::Casual),
    ("web.telegram.org", AppCategory::Casual),
    ("messenger.com", AppCategory::Casual),
    ("signal.org", AppCategory::Casual),
    ("instagram.com", AppCategory::Casual),
    // [GRAIN] Discord moved here from WorkChat. It is overwhelmingly a casual
    // surface, so the formal register was tightening up text that should have
    // stayed loose. Flip it back if a workspace-heavy user disagrees — it is one
    // line, and the icon works from either category.
    ("discord.com", AppCategory::Casual),
    ("messages.google.com", AppCategory::Casual),
    ("web.skype.com", AppCategory::Casual),
    ("web.snapchat.com", AppCategory::Casual),
    ("element.io", AppCategory::Casual),
    ("beeper.com", AppCategory::Casual),
    // -- Social composers --
    ("x.com", AppCategory::Casual),
    ("twitter.com", AppCategory::Casual),
    ("reddit.com", AppCategory::Casual),
    ("bsky.app", AppCategory::Casual),
    ("threads.net", AppCategory::Casual),
    ("mastodon.social", AppCategory::Casual),
    ("linkedin.com", AppCategory::Casual),
    ("news.ycombinator.com", AppCategory::Casual),
    // -- Docs / notes / long-form --
    ("notion.so", AppCategory::Work),
    ("notion.site", AppCategory::Work),
    ("coda.io", AppCategory::Work),
    ("obsidian.md", AppCategory::Work),
    ("roamresearch.com", AppCategory::Work),
    ("workflowy.com", AppCategory::Work),
    ("evernote.com", AppCategory::Work),
    ("onenote.com", AppCategory::Work),
    ("dropbox.com", AppCategory::Work),
    ("medium.com", AppCategory::Work),
    ("substack.com", AppCategory::Work),
    ("ghost.io", AppCategory::Work),
    ("wordpress.com", AppCategory::Work),
    ("confluence.", AppCategory::Work),
    ("sharepoint.com", AppCategory::Work),
    ("quip.com", AppCategory::Work),
    ("hackmd.io", AppCategory::Work),
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
    /// `TextPattern` is available at all (editors, documents, page bodies).
    pub has_text_pattern: bool,
    pub control: ControlClass,
    /// `TextPattern` yielded a caret (or selection) to anchor on.
    pub has_caret: bool,
    /// Focus belongs to **Grain itself** (the pill, a panel, the settings
    /// window). Nothing can be concluded about someone else's paste from our
    /// own window, and Grain's surfaces expose no text affordance — so without
    /// this every focus steal by the pill would read as a missed paste.
    pub is_own_process: bool,
    /// The foreground GUI thread owns a **system caret** (`GetGUIThreadInfo`'s
    /// `hwndCaret`).
    ///
    /// This is the signal UI Automation misses. Terminal emulators, custom-drawn
    /// editors and older Win32 applications accept pasted text while exposing
    /// little or no UIA text pattern — judged on UIA alone they look like a
    /// missed paste, which would hold a transcript that landed perfectly well.
    /// A blinking caret is proof that an insertion point exists.
    pub has_native_caret: bool,
}

impl FocusFacts {
    /// Whether this element exposes **any** route by which typed or pasted text
    /// could enter it.
    ///
    /// The negation is the useful direction: an element offering no text
    /// pattern, no value, no text control type and no system caret cannot have
    /// received a paste. That is a statement about the element, not a guess
    /// about the application, which is what makes it usable as evidence.
    ///
    /// The two mechanisms are complementary rather than redundant: UI
    /// Automation covers web and modern frameworks, the system caret covers
    /// native and custom-drawn ones.
    pub fn has_any_text_affordance(&self) -> bool {
        self.has_text_edit_pattern
            || self.value_read_only.is_some()
            || self.has_text_pattern
            || self.has_native_caret
            || matches!(self.control, ControlClass::Edit | ControlClass::Document)
    }
}

/// A cheap composite identity for the focused element.
///
/// Not a true `RuntimeId` — that is a SAFEARRAY needing manual COM lifetime
/// handling for no benefit here. All this has to answer is "is the thing I am
/// looking at now the same thing the paste went to?", and a changed process,
/// control type or owning window answers that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FocusIdentity {
    pub process_id: i32,
    pub control_type: i32,
    pub native_window: isize,
}

/// Everything Paste Catch needs about the focused element, from one read.
#[derive(Debug, Clone, Default)]
pub struct FocusProbe {
    pub facts: FocusFacts,
    pub identity: FocusIdentity,
    /// The caret neighbourhood, when there is a caret to anchor on.
    pub caret: Option<CaretContext>,
    /// The element's value — read only when there is no caret, since
    /// single-line inputs expose `ValuePattern` but often no usable selection.
    pub value: Option<String>,
}

/// One read of the focused element for Paste Catch. `None` only when focus
/// itself could not be resolved, which is the genuinely inconclusive case.
pub fn read_focus_probe() -> Option<FocusProbe> {
    #[cfg(windows)]
    {
        uia::read_focus_probe()
    }
    #[cfg(not(windows))]
    {
        None
    }
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
/// unknown (the caller then keeps the generic [`AppCategory::Other`]).
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
pub(crate) fn host_matches(host: &str, pattern: &str) -> bool {
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
/// Covers the popular desktop apps; everything else is `Other` (no instruction).
/// Match is a substring/stem check so channel variants (`code`, `code - insiders`,
/// `WhatsApp`, `WhatsAppDesktop`) all resolve.
///
/// The app lists below stay SPLIT even where several now feed one profile —
/// editors and terminals are both `Technical`, messengers and social clients are
/// both `Casual`. Merging the lists too would throw away the record of what each
/// profile is made of, which is the only thing that makes re-splitting one later
/// a small change rather than an archaeology exercise.
fn category_for_exe(stem: &str) -> AppCategory {
    // IDEs / editors.
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
    const WORK_CHAT: &[&str] = &["slack", "teams", "ms-teams", "webex"];
    // Personal messengers. `discord` is here, not in work chat: the site table
    // already treats discord.com as casual, and the desktop app disagreeing with
    // the website about the same service is a bug nobody would think to look for.
    const PERSONAL_CHAT: &[&str] = &[
        "whatsapp",
        "messenger",
        "telegram",
        "signal",
        "wechat",
        "line",
        "viber",
        "imessage",
        "discord",
    ];
    // Social composers (native desktop clients).
    const SOCIAL: &[&str] = &["x", "twitter", "tweetdeck"];
    // Docs / notes.
    const DOCS: &[&str] = &[
        "notion", "obsidian", "winword", "onenote", "evernote", "bear", "typora", "logseq",
    ];

    // Short keys (≤3 chars, e.g. "wt", "zen", "arc", "tor", "min", "x") must match
    // the stem EXACTLY — substring-matching them would misfire on ordinary words
    // ("editor" contains "tor", "examine" contains "min"). Longer keys may match as
    // a substring so channel variants ("code - insiders", "whatsappdesktop") resolve.
    let hit = |set: &[&str]| {
        set.iter()
            .any(|k| stem == *k || (k.len() >= 4 && stem.contains(k)))
    };
    // Desktop AI assistants. Checked BEFORE the IDE list on purpose: "claude"
    // and "chatgpt" are prompt boxes wherever they run, and the editors that
    // embed an assistant (Cursor, Windsurf) are still editors — the window the
    // user dictates into there is a code buffer far more often than a chat.
    const AI_CHAT: &[&str] = &[
        "claude",
        "chatgpt",
        "perplexity",
        "msty",
        "lmstudio",
        "jan",
        "anythingllm",
        "ollama",
    ];
    if hit(AI_CHAT) {
        AppCategory::AiChat
    } else if hit(IDE) || hit(TERMINAL) {
        AppCategory::Technical
    } else if hit(EMAIL) {
        AppCategory::Email
    } else if hit(WORK_CHAT) || hit(DOCS) {
        AppCategory::Work
    } else if hit(PERSONAL_CHAT) || hit(SOCIAL) {
        AppCategory::Casual
    } else {
        // Browsers land here too. A browser is not a profile — "the user is in
        // Chrome" says nothing about whether they are writing an email or a
        // shell command — so an unresolved site gets no instruction rather than
        // a vague one. When the site IS resolved, `category_for_site` supplies
        // the profile, which is the path that matters for most people.
        AppCategory::Other
    }
}

/// Whether this executable is a web browser.
///
/// Separate from [`category_for_exe`] because browser-ness is a *detection*
/// fact, not a profile: it is what decides whether UI Automation is spun up to
/// read the address bar. It used to be inferred from `category == Browser`, and
/// deleting that variant without this would have silently switched off site
/// detection for everyone — the site table, the pill's website icons, and every
/// webmail user's Email profile all hang off this returning true.
fn is_browser_exe(stem: &str) -> bool {
    // Kept broad so URL/site awareness is browser-agnostic: Chromium forks and
    // Gecko/Firefox forks alike. The URL reader works off the accessibility
    // tree, not a per-browser rule.
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
    BROWSER
        .iter()
        .any(|k| stem == *k || (k.len() >= 4 && stem.contains(k)))
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
    /// The foreground process's AppUserModelID, for a packaged (MSIX / Store)
    /// app. `None` for classic Win32, which is the common case and not a failure.
    ///
    /// Read here rather than by each consumer because it comes off the SAME
    /// process handle `exe_path` does — so carrying it costs one extra query on
    /// a handle already open, and saves the pill's icon path a second
    /// `GetForegroundWindow`/`OpenProcess` round-trip of its own.
    ///
    /// It is the second half of app identity. A packaged app's `exe` is often a
    /// stub or a name several Store apps share, and it is the AppUserModelID that
    /// the Shell, the picker, and this all agree on.
    pub aumid: Option<String>,
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

impl ActiveContext {
    /// Whether a browser is in front but no address was read out of it.
    ///
    /// This is "ask me again in a moment", not "there is no site here". A
    /// browser builds its accessibility tree lazily, so the first read after a
    /// window switch legitimately comes back empty — Gecko is the usual culprit
    /// — and a moment later answers fine.
    ///
    /// The distinction matters because the two look identical from outside and
    /// deserve opposite treatment: a browser sitting on an *unsupported* site
    /// HAS a host, so it lands as `false` here and is left alone. Only the case
    /// where the answer has not arrived yet is worth asking twice.
    pub fn site_read_may_be_early(&self) -> bool {
        self.url_host.is_none() && is_browser_exe(&self.exe)
    }
}

/// Compose the final post-processing system prompt from up to four stages.
///
/// `spoken_instruction` is the **Prompt Record** layer: an instruction the user
/// dictated mid-recording (through Prompt Record), aimed at THIS transcript. It is
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
    // The user's edited instruction when there is one, otherwise the shipped
    // default. Read through `settings` rather than off the category so that what
    // the settings UI shows and what the model receives are the same string —
    // the whole point of surfacing these was that they stop being invisible.
    let soft = ctx.and_then(|c| instruction_for(c, settings));
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
    use super::{category_for_exe, is_browser_exe, ActiveContext, Confidence};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, MAX_PATH};
    use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
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

            let Some((exe_path, aumid)) = process_identity(pid) else {
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
            let is_browser = is_browser_exe(&exe);
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
                    .unwrap_or_else(|| if read_caret {
                        "none".into()
                    } else {
                        "off".into()
                    }),
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
                aumid,
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

    /// Both identities of `pid`: its full image path, and its AppUserModelID when
    /// it is a packaged app.
    ///
    /// One `OpenProcess` for the pair. They were read separately once — here and
    /// again in the pill's icon path — and the handle, not either query, is the
    /// expensive part. `PROCESS_QUERY_LIMITED_INFORMATION` is enough for both, so
    /// no elevation is needed for most apps.
    unsafe fn process_identity(pid: u32) -> Option<(String, Option<String>)> {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let aumid = packaged_aumid(handle);
        let _ = CloseHandle(handle);
        res.ok()?;
        Some((String::from_utf16_lossy(&buf[..len as usize]), aumid))
    }

    /// The AppUserModelID of a packaged (MSIX / Store) process, or `None`.
    ///
    /// `APPMODEL_ERROR_NO_APPLICATION` — a classic Win32 process — is the normal
    /// answer and is reported as `None` rather than logged, because it says
    /// nothing has gone wrong. Note that this is the *package* identity: a
    /// desktop app that set an explicit AppUserModelID on itself is still `None`
    /// here, which is exactly why the catalogue keys desktop apps on their exe.
    unsafe fn packaged_aumid(handle: HANDLE) -> Option<String> {
        // AppUserModelIDs are bounded well below this; the call reports the
        // needed length, but one generous buffer avoids the two-call dance.
        let mut len = 512u32;
        let mut buf = vec![0u16; len as usize];
        GetApplicationUserModelId(
            handle,
            &mut len,
            Some(windows::core::PWSTR(buf.as_mut_ptr())),
        )
        .ok()
        .ok()?;
        let len = (len as usize).min(buf.len()).saturating_sub(1);
        let s = String::from_utf16_lossy(&buf[..len]);
        (!s.is_empty()).then_some(s)
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
        FocusFacts, FocusIdentity, FocusProbe, MAX_CARET_CHARS,
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

    /// Facts + caret (+ value only when there is no caret) in ONE pass.
    ///
    /// One `GetFocusedElement` for everything: Paste Catch needs all three to
    /// reach a verdict, and asking twice would both cost a second cross-process
    /// round trip and risk the two reads seeing different focus.
    pub(in crate::context_detect) fn read_focus_probe() -> Option<FocusProbe> {
        unsafe {
            let _com = ComGuard::init();
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let el = automation.GetFocusedElement().ok()?;

            let mut facts = FocusFacts {
                is_password: is_password(&el),
                ..FocusFacts::default()
            };
            let mut identity = FocusIdentity::default();
            if facts.is_password {
                return Some(FocusProbe {
                    facts,
                    identity,
                    caret: None,
                    value: None,
                });
            }
            fill_patterns(&el, &mut facts, &mut identity);

            let caret = read_caret(&el).filter(|c| !c.is_empty());
            // Only when there is no caret to anchor on: single-line inputs
            // expose ValuePattern but often no usable selection range.
            let value = if caret.is_none() {
                read_value(&el)
            } else {
                None
            };
            Some(FocusProbe {
                facts,
                identity,
                caret,
                value,
            })
        }
    }

    /// Whether the foreground GUI thread owns a system caret.
    ///
    /// Deliberately asked of the foreground THREAD rather than the element: the
    /// caret is a thread-level concept, and this is exactly the case where the
    /// element itself tells us nothing. One Win32 call, no COM.
    unsafe fn foreground_has_caret() -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
        };
        let Some(hwnd) = super::windows_impl::foreground_window() else {
            return false;
        };
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        if thread_id == 0 {
            return false;
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread_id, &mut info).is_ok() && !info.hwndCaret.is_invalid()
    }

    /// Pattern availability, control class and identity — one pass over the
    /// focused element.
    unsafe fn fill_patterns(
        el: &IUIAutomationElement,
        facts: &mut FocusFacts,
        identity: &mut FocusIdentity,
    ) {
        identity.process_id = el.CurrentProcessId().unwrap_or(0);
        identity.native_window = el
            .CurrentNativeWindowHandle()
            .map(|hwnd| hwnd.0 as isize)
            .unwrap_or(0);
        facts.is_own_process = identity.process_id as u32 == std::process::id();
        facts.has_native_caret = foreground_has_caret();
        facts.has_text_edit_pattern = el
            .GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId)
            .is_ok();
        facts.value_read_only = el
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .ok()
            .and_then(|vp| vp.CurrentIsReadOnly().ok())
            .map(|b| b.as_bool());

        let text_pattern = el
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok();
        facts.has_text_pattern = text_pattern.is_some();

        if let Ok(control_type) = el.CurrentControlType() {
            identity.control_type = control_type.0;
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

        facts.has_caret = text_pattern
            .and_then(|tp| tp.GetSelection().ok())
            .and_then(|sel| sel.GetElement(0).ok())
            .is_some();
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
mod focus_target_tests {
    use super::*;

    fn facts() -> FocusFacts {
        FocusFacts::default()
    }

    #[test]
    fn password_field_counts_as_landed_and_is_never_held() {
        let f = FocusFacts {
            is_password: true,
            // Even with every other signal screaming "not editable", a password
            // box must never route the transcript into a held clipboard.
            value_read_only: Some(true),
            control: ControlClass::NonText,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::Editable);
    }

    #[test]
    fn text_edit_pattern_is_decisive() {
        let f = FocusFacts {
            has_text_edit_pattern: true,
            control: ControlClass::Other,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::Editable);
    }

    #[test]
    fn read_only_value_is_positive_evidence_of_a_miss() {
        let f = FocusFacts {
            value_read_only: Some(true),
            control: ControlClass::Edit,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::NotEditable);
    }

    #[test]
    fn writable_value_is_editable() {
        let f = FocusFacts {
            value_read_only: Some(false),
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::Editable);
    }

    #[test]
    fn edit_control_without_patterns_is_editable() {
        let f = FocusFacts {
            control: ControlClass::Edit,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::Editable);
    }

    #[test]
    fn caret_less_document_is_a_read_only_surface() {
        // PDF viewer, mail preview, rendered reader.
        let f = FocusFacts {
            control: ControlClass::Document,
            has_caret: false,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::NotEditable);
    }

    #[test]
    fn document_with_a_caret_stays_ambiguous() {
        // A selectable web page and a real editor both present a caret. This is
        // the case we must NOT guess wrong about, so it falls through to
        // post-paste verification instead.
        let f = FocusFacts {
            control: ControlClass::Document,
            has_caret: true,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::Unknown);
    }

    #[test]
    fn leaf_widgets_are_positive_evidence() {
        let f = FocusFacts {
            control: ControlClass::NonText,
            ..facts()
        };
        assert_eq!(classify(f), FocusTarget::NotEditable);
    }

    #[test]
    fn container_control_types_never_report_not_editable() {
        // `Pane`/`Window`/`Group`/`Custom` map to `Other`. Electron and canvas
        // editors report those for perfectly editable surfaces, so suppressing
        // a paste on that basis would break real apps.
        assert_eq!(
            classify(FocusFacts {
                control: ControlClass::Other,
                ..facts()
            }),
            FocusTarget::Unknown
        );
    }

    #[test]
    fn a_failed_read_yields_no_evidence() {
        // `read_focus_facts` returns the default on every failure path, and the
        // default must never be actionable.
        assert_eq!(classify(FocusFacts::default()), FocusTarget::Unknown);
    }
}

#[cfg(test)]
mod site_table_tests {
    use super::*;

    /// [GRAIN] `SITE_TABLE` is matched in order and [`host_matches`] accepts
    /// subdomains, so a general pattern placed above a specific one silently
    /// swallows it — the specific row still compiles, still looks right, and
    /// never fires. That is a wrong post-processing profile with no error
    /// anywhere, which is exactly the kind of bug nobody finds by reading.
    ///
    /// Every host in the table must therefore resolve to its OWN category.
    #[test]
    fn no_site_entry_is_shadowed_by_an_earlier_pattern() {
        for (i, (host, category)) in SITE_TABLE.iter().enumerate() {
            // Bare prefixes (`jira.`, `confluence.`) are patterns, not hosts —
            // they match by prefix and cannot be probed as a hostname.
            if host.ends_with('.') {
                continue;
            }
            let resolved = category_for_site(host)
                .unwrap_or_else(|| panic!("row {i} ({host}) does not resolve at all"));
            assert_eq!(
                resolved, *category,
                "row {i} ({host}) is shadowed by an earlier, more general pattern \
                 — move it above whichever row is swallowing it"
            );
        }
    }

    /// The subdomain rule is what makes one row cover a whole service, and also
    /// what makes shadowing possible — so pin both halves of it.
    #[test]
    fn a_pattern_covers_its_subdomains_but_not_a_lookalike_domain() {
        assert_eq!(category_for_site("slack.com"), Some(AppCategory::Work));
        assert_eq!(
            category_for_site("app.slack.com"),
            Some(AppCategory::Work),
            "a subdomain must inherit its parent's row"
        );
        // The attack this guards: a domain that merely ENDS WITH a trusted one
        // must not inherit its profile.
        assert_eq!(category_for_site("notslack.com"), None);
        assert_eq!(category_for_site("slack.com.evil.test"), None);
    }

    /// Bare `outlook.com` sits below three more specific Outlook hosts. None is
    /// a subdomain of it, but the ordering is easy to get wrong on the next
    /// addition, so state the expectation rather than leaving it to the comment.
    #[test]
    fn the_specific_outlook_hosts_still_resolve_alongside_the_bare_one() {
        for host in [
            "outlook.office.com",
            "outlook.office365.com",
            "outlook.live.com",
            "outlook.com",
        ] {
            assert_eq!(category_for_site(host), Some(AppCategory::Email), "{host}");
        }
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
            aumid: None,
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
        assert_eq!(category_for_exe("code"), AppCategory::Technical);
        assert_eq!(category_for_exe("cursor"), AppCategory::Technical);
        assert_eq!(category_for_exe("outlook"), AppCategory::Email);
        assert_eq!(category_for_exe("slack"), AppCategory::Work);
        assert_eq!(category_for_exe("whatsapp"), AppCategory::Casual);
        assert_eq!(category_for_exe("notion"), AppCategory::Work);
        assert_eq!(category_for_exe("chrome"), AppCategory::Other);
        assert_eq!(category_for_exe("some_unknown_app"), AppCategory::Other);
    }

    /// Terminals are no longer IDEs: a shell wants a command, an editor wants
    /// code, and the two soft lines say different things.
    #[test]
    fn terminals_split_from_ides() {
        assert_eq!(category_for_exe("pwsh"), AppCategory::Technical);
        assert_eq!(category_for_exe("wt"), AppCategory::Technical);
        assert_eq!(category_for_exe("alacritty"), AppCategory::Technical);
        assert_eq!(category_for_exe("ghostty"), AppCategory::Technical);
        // …and the editors stayed put.
        assert_eq!(category_for_exe("code"), AppCategory::Technical);
        assert_eq!(category_for_exe("nvim"), AppCategory::Technical);
    }

    /// The hole this phase exists to close: a browser tab now resolves to what
    /// the SITE is, not to "a browser".
    #[test]
    fn site_table_resolves_webapps_to_real_categories() {
        assert_eq!(category_for_site("mail.google.com"), Some(AppCategory::Email));
        assert_eq!(category_for_site("claude.ai"), Some(AppCategory::AiChat));
        assert_eq!(category_for_site("github.com"), Some(AppCategory::Technical));
        assert_eq!(category_for_site("linear.app"), Some(AppCategory::Work));
        assert_eq!(category_for_site("app.slack.com"), Some(AppCategory::Work));
        assert_eq!(category_for_site("docs.google.com"), Some(AppCategory::Work));
        assert_eq!(category_for_site("x.com"), Some(AppCategory::Casual));
        // Unknown sites resolve to nothing, so the caller keeps `Browser`.
        assert_eq!(category_for_site("example.com"), None);
    }

    /// Subdomains inherit, and `www.` is irrelevant.
    #[test]
    fn site_matching_accepts_subdomains_and_strips_www() {
        assert_eq!(category_for_site("www.github.com"), Some(AppCategory::Technical));
        assert_eq!(category_for_site("gist.github.com"), Some(AppCategory::Technical));
        assert_eq!(category_for_site("acme.atlassian.net"), Some(AppCategory::Work));
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
        assert_eq!(category_for_site("jira.acme.com"), Some(AppCategory::Work));
        assert_eq!(category_for_site("confluence.acme.com"), Some(AppCategory::Work));
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
        assert_eq!(category_for_site("chat.google.com"), Some(AppCategory::Work));
        assert_eq!(category_for_site("docs.google.com"), Some(AppCategory::Work));
        assert_eq!(category_for_site("gemini.google.com"), Some(AppCategory::AiChat));
    }

    /// A single-line field must not get a trailing period bolted on. This is the
    /// daily papercut the field detection exists for.
    #[test]
    fn single_line_field_suppresses_terminal_punctuation() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let mut c = ctx("chrome", AppCategory::Other);
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
        let mut c = ctx("winword", AppCategory::Work);
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
        let mut c = ctx("winword", AppCategory::Work);
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
        let mut c = ctx("winword", AppCategory::Work);
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
        let mut c = ctx("winword", AppCategory::Work);
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
        let out = compose_prompt("BASE", &s, Some(&ctx("code", AppCategory::Technical)), None);
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
        let mut c = ctx("winword", AppCategory::Work);
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

        let mut main = ctx("chrome", AppCategory::Work);
        main.region = Some("main".into());
        assert!(!compose_prompt("BASE", &s, Some(&main), None).contains("page region"));

        let mut side = ctx("chrome", AppCategory::Work);
        side.region = Some("complementary".into());
        assert!(compose_prompt("BASE", &s, Some(&side), None).contains("page region: complementary"));
    }

    /// Each profile absorbed several older, narrower lines. These are the rules
    /// whose loss is immediately visible in the output, so name them
    /// individually — a generic "contains a boundary word" check would pass
    /// while the rule that actually matters had been rewritten away.
    ///
    /// Matched on the DISTINCTIVE noun rather than the exact sentence, so the
    /// wording can keep improving without the test becoming a copy of it.
    #[test]
    fn merging_the_profiles_did_not_drop_a_guard_that_was_load_bearing() {
        let technical = AppCategory::Technical.default_instruction().unwrap();
        // From the old Terminal line: a command is not a sentence, and a
        // sentence-cased command with a full stop is wrong on sight.
        assert!(technical.contains("unpunctuated"), "{technical}");
        assert!(technical.contains("lowercase"), "{technical}");

        let work = AppCategory::Work.default_instruction().unwrap();
        // From the old Docs line — the merge most at risk of being swallowed by
        // chat-shaped advice.
        assert!(work.contains("structure"), "{work}");

        let casual = AppCategory::Casual.default_instruction().unwrap();
        assert!(casual.contains("voice"), "{casual}");

        // The whole reason AiChat is a profile again: a cleanup model handed a
        // dictated question answers it instead of writing it down.
        let ai = AppCategory::AiChat.default_instruction().unwrap();
        assert!(ai.contains("answer nothing"), "{ai}");
        assert!(ai.contains("question"), "{ai}");
    }

    /// Every profile must bound itself: it may shape tone, never impose
    /// structure. Since the rewrite to positive framing the boundary is usually
    /// phrased as a condition ("only if it was dictated") rather than a
    /// prohibition, so both forms count — what must not happen is a profile
    /// with no boundary at all, which is how a tone hint becomes hard
    /// formatting.
    #[test]
    fn instructions_stay_soft_and_bounded() {
        for category in [
            AppCategory::Email,
            AppCategory::Work,
            AppCategory::Casual,
            AppCategory::Technical,
            AppCategory::AiChat,
        ] {
            let line = category.default_instruction().expect("profile adds context");
            let lower = line.to_ascii_lowercase();
            assert!(
                ["only if", "only what", "nothing", "leave "]
                    .iter()
                    .any(|guard| lower.contains(guard)),
                "{category:?} has no boundary clause: {line}"
            );
            // Rides on every dictation, and instruction-following degrades with
            // length long before any context limit — so this ceiling is about
            // accuracy, not cost. ~4 bytes/token.
            assert!(
                line.len() <= 340,
                "{category:?} instruction is {} bytes, too long to be followed reliably",
                line.len()
            );
        }
        assert!(AppCategory::Other.default_instruction().is_none());
    }

    /// `Other` is the absence of a profile, so it must not be addressable as
    /// one — otherwise a stored override could give browsers-on-unknown-sites an
    /// instruction through the back door, which is the thing being removed.
    #[test]
    fn other_is_not_an_editable_profile() {
        assert!(AppCategory::Other.profile_id().is_none());
        assert!(AppCategory::from_profile_id("other").is_none());
        assert!(AppCategory::from_profile_id("browser").is_none());
        for id in PROFILE_IDS {
            let category = AppCategory::from_profile_id(id).expect("id must round-trip");
            assert_eq!(category.profile_id(), Some(id));
        }
    }

    /// An edited profile must change what the model receives — the point of
    /// surfacing these was that the shown text IS the sent text.
    #[test]
    fn an_edited_instruction_replaces_the_default_in_the_prompt() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_profile_instructions
            .push(crate::settings::ContextProfileInstruction {
                id: "email".into(),
                instruction: "Write it like a pirate.".into(),
            });
        let c = ctx("outlook", AppCategory::Email);
        let out = compose_prompt("BASE", &s, Some(&c), None);
        assert!(out.contains("Write it like a pirate."));
        assert!(!out.contains("An email composer."));
        // An untouched profile is unaffected by its neighbour's edit.
        let t = ctx("code", AppCategory::Technical);
        assert!(compose_prompt("BASE", &s, Some(&t), None).contains("exactly as spoken"));
    }

    fn custom(id: &str, instruction: &str, kind: &str, value: &str) -> CustomContextProfile {
        CustomContextProfile {
            id: id.into(),
            title: id.into(),
            instruction: instruction.into(),
            targets: vec![crate::settings::ContextProfileTarget {
                kind: kind.into(),
                value: value.into(),
            }],
        }
    }

    /// The point of a custom profile: naming a surface means Grain's guess is
    /// wrong for it, so the user's text wins outright.
    #[test]
    fn a_custom_profile_overrides_the_builtin_category_for_its_app() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("p1", "Speak only in haiku.", "application", "code"));

        let claimed = ctx("code", AppCategory::Technical);
        let out = compose_prompt("BASE", &s, Some(&claimed), None);
        assert!(out.contains("Speak only in haiku."));
        assert!(!out.contains("exactly as spoken"), "built-in leaked through");

        // A different app in the same built-in category is untouched.
        let other = ctx("nvim", AppCategory::Technical);
        assert!(compose_prompt("BASE", &s, Some(&other), None).contains("exactly as spoken"));
    }

    /// A profile with exactly one target is how "this app gets its own
    /// instruction" is expressed — it needs no separate concept, so guard that
    /// the single-target case is not treated as incomplete.
    #[test]
    fn a_profile_with_one_application_is_valid() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("solo", "Terse.", "application", "figma"));
        let c = ctx("figma", AppCategory::Other);
        // `Other` normally adds nothing at all; the profile is what gives this
        // surface an instruction.
        assert!(compose_prompt("BASE", &s, Some(&c), None).contains("Terse."));
    }

    /// A packaged (Store / MSIX) app is named by its AppUserModelID, because it
    /// is the only identity it has — there is no shortcut behind it and so no
    /// executable for the picker to offer. Matching has to accept that second
    /// form or every packaged app a user picks would save fine and never fire.
    #[test]
    fn a_packaged_application_target_matches_on_its_appusermodelid() {
        const NOTEPAD: &str = "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App";
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("notes", "Plain text only.", "application", NOTEPAD));

        let mut c = ctx("notepad", AppCategory::Other);
        c.aumid = Some(NOTEPAD.to_string());
        assert!(compose_prompt("BASE", &s, Some(&c), None).contains("Plain text only."));

        // …and a different packaged app with the same-looking exe stem does not
        // inherit it, which is the whole reason the AppUserModelID is stored.
        let mut other = ctx("notepad", AppCategory::Other);
        other.aumid = Some("SomeVendor.NotepadClone_abc123!App".to_string());
        assert!(!compose_prompt("BASE", &s, Some(&other), None).contains("Plain text only."));
    }

    /// The two identities must not bleed into one another: a desktop app is
    /// stored as an exe stem and a packaged one as an AppUserModelID, and
    /// comparing a target against both is only safe while neither shape can
    /// satisfy the wrong side.
    #[test]
    fn an_exe_target_never_matches_a_packaged_window_by_accident() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("editor", "Syntax intact.", "application", "code"));

        let mut c = ctx("someotherapp", AppCategory::Other);
        c.aumid = Some("Contoso.Code_8wekyb3d8bbwe!App".to_string());
        assert!(!compose_prompt("BASE", &s, Some(&c), None).contains("Syntax intact."));
    }

    /// A site claim beats an app claim, mirroring the built-in rule: "Chrome"
    /// describes the window, the host describes the work. Without this, one
    /// profile claiming `chrome` would swallow every site profile.
    #[test]
    fn a_custom_site_outranks_a_custom_app_on_the_same_window() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("browser", "APP RULE", "application", "chrome"));
        s.context_custom_profiles
            .push(custom("site", "SITE RULE", "website", "figma.com"));

        let mut c = ctx("chrome", AppCategory::Other);
        c.url_host = Some("figma.com".into());
        c.confidence = Confidence::Exact;
        let out = compose_prompt("BASE", &s, Some(&c), None);
        assert!(out.contains("SITE RULE"));
        assert!(!out.contains("APP RULE"));
    }

    /// Same confidence bar as the built-in site table: a host merely scraped
    /// from the window may name the site but may not claim the surface, because
    /// on a multi-tab window it can belong to a tab the user is not typing into.
    #[test]
    fn a_guessed_host_cannot_trigger_a_custom_site_profile() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("site", "SITE RULE", "website", "figma.com"));
        let mut c = ctx("chrome", AppCategory::Other);
        c.url_host = Some("figma.com".into());
        c.confidence = Confidence::Probable;
        assert!(!compose_prompt("BASE", &s, Some(&c), None).contains("SITE RULE"));
    }

    /// Subdomains inherit, so claiming `figma.com` covers the app subdomain —
    /// and a lookalike domain still does not match.
    #[test]
    fn custom_site_targets_follow_the_registry_host_rule() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("site", "SITE RULE", "website", "figma.com"));
        let mut c = ctx("chrome", AppCategory::Other);
        c.confidence = Confidence::Exact;

        c.url_host = Some("www.figma.com".into());
        assert!(compose_prompt("BASE", &s, Some(&c), None).contains("SITE RULE"));
        c.url_host = Some("notfigma.com".into());
        assert!(!compose_prompt("BASE", &s, Some(&c), None).contains("SITE RULE"));
    }

    /// The card's icon stack is only as good as these: each must be a real
    /// host that resolves back to the profile claiming it, or the card shows an
    /// icon for a site the profile does not apply to.
    #[test]
    fn every_profile_offers_real_sample_hosts_of_its_own() {
        for id in PROFILE_IDS {
            let category = AppCategory::from_profile_id(id).unwrap();
            let samples = sample_sites(category, 3);
            assert!(
                !samples.is_empty(),
                "{id} has no sample site for its card icons"
            );
            for host in samples {
                // A prefix wildcard names no fetchable host, so it must never
                // reach the card.
                assert!(!host.ends_with('.'), "{id}: {host} is a wildcard pattern");
                assert!(host.contains('.'), "{id}: {host} is not a host");
                assert_eq!(
                    category_for_site(&host),
                    Some(category),
                    "{id}: {host} does not resolve back to this profile"
                );
            }
        }
    }

    /// A site named in a profile becomes a supported site, so the pill can show
    /// its favicon. One answer to "does Grain know this site", shared by
    /// post-processing and the icon path.
    #[test]
    fn a_custom_site_becomes_a_supported_site_for_icons() {
        let mut s = AppSettings::default();
        assert!(!is_supported_site("figma.com", &s));
        s.context_custom_profiles
            .push(custom("site", "x", "website", "figma.com"));
        assert!(is_supported_site("figma.com", &s));
        assert!(is_supported_site("app.figma.com", &s));
        assert!(!is_supported_site("notfigma.com", &s));
        // Built-ins still resolve, obviously.
        assert!(is_supported_site("github.com", &s));
    }

    /// An emptied custom instruction still wins. "Say nothing for this one app"
    /// is a real thing to want, and falling back to the built-in would silently
    /// ignore the profile the user made.
    #[test]
    fn an_empty_custom_instruction_silences_the_surface() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_custom_profiles
            .push(custom("mute", "  ", "application", "code"));
        let c = ctx("code", AppCategory::Technical);
        let out = compose_prompt("BASE", &s, Some(&c), None);
        assert!(!out.contains("exactly as spoken"));
    }

    /// Clearing an instruction is a legitimate choice, not a broken override:
    /// the profile then adds nothing, exactly like `Other`.
    #[test]
    fn an_instruction_emptied_by_the_user_adds_nothing() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.context_profile_instructions
            .push(crate::settings::ContextProfileInstruction {
                id: "casual".into(),
                instruction: "   ".into(),
            });
        assert!(AppCategory::Casual.instruction(&s).is_none());
    }

    #[test]
    fn disabled_returns_base_untouched() {
        let s = AppSettings::default(); // context_awareness_enabled = false
        let base = "BASE PROMPT ${output}";
        assert_eq!(
            compose_prompt(base, &s, Some(&ctx("code", AppCategory::Technical)), None),
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
        let out = compose_prompt(base, &s, Some(&ctx("code", AppCategory::Technical)), None);
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
            Some(&ctx("code", AppCategory::Technical)),
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
