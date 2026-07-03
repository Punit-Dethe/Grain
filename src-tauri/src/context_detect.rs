//! [GRAIN] Context awareness — detect the foreground app/site and compose the
//! three-stage post-processing system prompt.
//!
//! # The three stages
//! 1. **BASE** — the user's selected post-processing prompt (General/Email/Coding
//!    or a custom one). Always present; unchanged behavior.
//! 2. **CONTEXT (soft)** — an automatic, ≤2-line nudge derived from the detected
//!    app *category* (tone + vocabulary). Never restructures or hard-formats.
//! 3. **MODE (hard)** — a user-defined [`AppMode`] prompt, injected ONLY when its
//!    matcher hits the active app/site. This is where hard formatting lives, and
//!    only because the user asked for it.
//!
//! This is a **zero-overhead inline interceptor**, not a new engine: detection is
//! one cheap OS call made ONCE per finalized transcript (never per rolling chunk),
//! right before LLM post-processing, and composition is pure string work. When
//! context awareness is off — or nothing is detected and no mode matches — the
//! base prompt is returned untouched, so the common path is exactly today's.
//!
//! Detection is Windows-only for now; other platforms return `None`, degrading
//! cleanly to BASE-only behavior. Browser URL/site detection is a later increment
//! (needs UI Automation); until then browsers get the generic `Browser` category.

use grain_core::{AppMatch, AppMode, AppSettings};

/// Coarse app category driving the automatic SOFT context line. Deliberately a
/// small, robust bucket set (à la the incumbents' 4–8 categories) rather than a
/// per-app rule table: unknown apps fall to [`AppCategory::Other`], which adds no
/// context at all, so behavior degrades safely for the long tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCategory {
    /// Code editors, IDEs, and terminals — technical vocabulary, keep jargon.
    Ide,
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
    /// A web browser with no site-level refinement yet: tone-neutral light cleanup.
    Browser,
    /// Anything unrecognized — no soft context is added.
    Other,
}

impl AppCategory {
    /// The SOFT context line for this category, or `None` when nothing should be
    /// added (`Other`). Kept to ≤2 sentences and explicitly non-restructuring so
    /// it stays token-cheap and honors the "no hard formatting" constraint.
    fn soft_line(self) -> Option<&'static str> {
        Some(match self {
            AppCategory::Ide => "It is a code editor / IDE / terminal. Preserve technical terms, identifiers, and library/framework names exactly (e.g. Tauri, Rust, useEffect) — do NOT 'correct' jargon into ordinary English — and wrap code-like tokens in backticks. Keep it terse.",
            AppCategory::Email => "It is an email composer. Use a slightly more polished, professional tone, but do NOT add a subject line, greeting, or sign-off and do NOT reformat into an email layout unless the user dictated it.",
            AppCategory::WorkChat => "It is a work chat (e.g. Slack/Teams). Keep it professional but concise and conversational; do not add greetings or restructure into formal paragraphs.",
            AppCategory::PersonalChat => "It is a casual personal messenger. Keep the user's own casual tone, slang, and phrasing; do light cleanup only and do not formalize.",
            AppCategory::Social => "It is a social-post composer. Keep it casual and punchy in the user's own voice; do not add hashtags, emoji, or restructure unless the user dictated it.",
            AppCategory::Docs => "It is a document/notes editor. Clean, readable prose is welcome, but preserve the user's wording and structure; do not impose headings or lists unless dictated.",
            AppCategory::Browser => "It is a text field in a web browser. Apply light, tone-neutral cleanup and match the style the user is already writing in.",
            AppCategory::Other => return None,
        })
    }
}

/// Map an executable stem (lowercased, no extension) to a coarse [`AppCategory`].
/// Covers the popular desktop apps; everything else is `Other` (no soft context).
/// Match is a substring/stem check so channel variants (`code`, `code - insiders`,
/// `WhatsApp`, `WhatsAppDesktop`) all resolve.
fn category_for_exe(stem: &str) -> AppCategory {
    // IDEs / editors / terminals.
    const IDE: &[&str] = &[
        "code", "cursor", "windsurf", "devenv", "idea64", "idea", "pycharm64",
        "pycharm", "webstorm64", "webstorm", "goland64", "clion64", "rider64",
        "rustrover64", "phpstorm64", "sublime_text", "zed", "nvim", "vim",
        "windowsterminal", "wt", "powershell", "pwsh", "cmd", "alacritty",
        "wezterm-gui", "wezterm", "kitty", "conemu", "hyper",
    ];
    // Email clients.
    const EMAIL: &[&str] = &["outlook", "thunderbird", "hmaildesktop", "mailspring", "spark"];
    // Work chat.
    const WORK_CHAT: &[&str] = &["slack", "teams", "ms-teams", "webex", "discord"];
    // Personal messengers.
    const PERSONAL_CHAT: &[&str] = &[
        "whatsapp", "messenger", "telegram", "signal", "wechat", "line",
        "viber", "imessage",
    ];
    // Social composers (native desktop clients).
    const SOCIAL: &[&str] = &["x", "twitter", "tweetdeck"];
    // Docs / notes.
    const DOCS: &[&str] = &[
        "notion", "obsidian", "winword", "onenote", "evernote", "bear",
        "typora", "logseq",
    ];
    // Browsers — kept broad so URL/site awareness is browser-agnostic. Covers
    // Chromium forks and Gecko/Firefox forks; the URL reader itself works off the
    // accessibility tree, not a per-browser rule.
    const BROWSER: &[&str] = &[
        "chrome", "msedge", "firefox", "brave", "opera", "operagx", "vivaldi",
        "arc", "browser", "chromium", "zen", "librewolf", "waterfox", "floorp",
        "mullvad", "palemoon", "seamonkey", "thorium", "yandex", "maxthon",
        "midori", "epic", "min", "sidekick", "wavebox", "falkon", "qutebrowser",
        "ungoogled", "duckduckgo", "tor",
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
    "the", "and", "you", "that", "was", "for", "are", "with", "his", "they",
    "this", "have", "from", "one", "had", "but", "not", "what", "all", "were",
    "when", "your", "can", "said", "there", "use", "each", "which", "she", "how",
    "their", "will", "other", "about", "out", "many", "then", "them", "these",
    "some", "her", "would", "make", "like", "him", "into", "time", "has", "look",
    "two", "more", "write", "see", "number", "way", "could", "people", "than",
    "first", "water", "been", "call", "who", "its", "now", "find", "long", "down",
    "day", "did", "get", "come", "made", "may", "part", "over", "new", "sound",
    "take", "only", "little", "work", "know", "place", "year", "live", "back",
    "give", "most", "very", "after", "thing", "our", "just", "name", "good",
    "sentence", "man", "think", "say", "great", "where", "help", "through",
    "much", "before", "line", "right", "too", "mean", "old", "any", "same",
    "tell", "boy", "follow", "came", "want", "show", "also", "around", "form",
    "three", "small", "set", "put", "end", "does", "another", "well", "large",
    "must", "big", "even", "such", "because", "turn", "here", "why", "ask",
    "went", "men", "read", "need", "land", "different", "home", "move", "try",
    "kind", "hand", "picture", "again", "change", "off", "play", "spell", "air",
    "away", "animal", "house", "point", "page", "letter", "mother", "answer",
    "found", "study", "still", "learn", "should", "america", "world", "high",
    "every", "near", "add", "food", "between", "own", "below", "country",
    "plant", "last", "school", "father", "keep", "tree", "never", "start",
    "city", "earth", "eye", "light", "thought", "head", "under", "story", "saw",
    "left", "few", "while", "along", "might", "close", "something", "seem",
    "next", "hard", "open", "example", "begin", "life", "always", "those",
    "both", "paper", "together", "got", "group", "often", "run", "important",
    "until", "children", "side", "feet", "car", "mile", "night", "walk", "white",
    "sea", "began", "grow", "took", "river", "four", "carry", "state", "once",
    "book", "hear", "stop", "without", "second", "later", "miss", "idea",
    "enough", "eat", "face", "watch", "far", "really", "almost", "let", "above",
    "girl", "sometimes", "mountain", "cut", "young", "talk", "soon", "list",
    "song", "being", "leave", "family", "it's", "please", "thanks", "hey", "hi",
    "yeah", "okay", "just", "going", "really", "actually", "basically",
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
        let intentional =
            internal_upper || has_underscore || has_digit || first_upper || all_upper;
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
    /// Executable stem, lowercased, no extension (the [`AppMatch::Process`] key).
    pub exe: String,
    pub category: AppCategory,
    /// Browser address-bar host, when the foreground app is a browser and UI
    /// Automation resolved it (e.g. `mail.google.com`). `None` otherwise.
    pub url_host: Option<String>,
    /// Unique non-dictionary tokens read from the focused field (proper nouns,
    /// identifiers, library names) — an ADDITIVE bias hint, never raw text. Empty
    /// unless the nearby-terms opt-in is on and something was found.
    pub nearby_terms: Vec<String>,
}

/// True if `mode` targets the given context. `Process` compares exe stems
/// case-insensitively; `UrlHost` matches by dot-aware host suffix so a bare
/// `mail.google.com` also fires on any sub-host of it.
pub fn mode_matches(mode: &AppMode, ctx: &ActiveContext) -> bool {
    match &mode.matcher {
        AppMatch::Process(p) => {
            let want = p.trim().trim_end_matches(".exe").to_ascii_lowercase();
            !want.is_empty() && ctx.exe == want
        }
        AppMatch::UrlHost(h) => {
            let want = h.trim().trim_start_matches("www.").to_ascii_lowercase();
            match &ctx.url_host {
                Some(host) if !want.is_empty() => {
                    let host = host.trim_start_matches("www.");
                    host == want || host.ends_with(&format!(".{want}"))
                }
                _ => false,
            }
        }
    }
}

/// The first enabled mode whose matcher hits `ctx`, if any.
fn matching_mode<'a>(settings: &'a AppSettings, ctx: &ActiveContext) -> Option<&'a AppMode> {
    settings
        .app_modes
        .iter()
        .find(|m| m.enabled && mode_matches(m, ctx))
}

/// Compose the final post-processing system prompt from the three stages.
///
/// Returns `base` unchanged when nothing applies (context off, no detection, and
/// no matching mode), so the common path is byte-for-byte today's behavior. When
/// context or a mode applies, a compact preamble is prepended (NOT appended — so
/// it precedes the transcript in both the structured and legacy `${output}`
/// paths) framing the layers and their priority.
pub fn compose_prompt(base: &str, settings: &AppSettings, ctx: Option<&ActiveContext>) -> String {
    if !settings.context_awareness_enabled {
        return base.to_string();
    }
    let Some(ctx) = ctx else {
        return base.to_string();
    };

    let soft = ctx.category.soft_line();
    let mode = matching_mode(settings, ctx);
    let has_terms = !ctx.nearby_terms.is_empty();
    if soft.is_none() && mode.is_none() && !has_terms {
        return base.to_string(); // nothing to add — untouched.
    }

    let mut pre = String::with_capacity(base.len() + 512);
    pre.push_str("[Context awareness]\n");
    pre.push_str(&format!(
        "The user is dictating into \"{}\".",
        ctx.app_name.trim()
    ));
    if let Some(host) = &ctx.url_host {
        pre.push_str(&format!(" (website: {host})"));
    }
    pre.push('\n');
    if let Some(line) = soft {
        pre.push_str("Soft context (tone/vocabulary only, never restructure): ");
        pre.push_str(line);
        pre.push('\n');
    }
    if let Some(m) = mode {
        pre.push_str(
            "User formatting instructions for this app (HIGHEST priority — follow exactly): ",
        );
        pre.push_str(m.prompt.trim());
        pre.push('\n');
    }
    if has_terms {
        // Additive, LOW authority: only fix a term to one of these spellings when
        // the transcript clearly meant it; otherwise ignore. Never insert them.
        pre.push_str(
            "Nearby terms the user may be referring to — use ONLY to correct the \
             spelling of a word already in the transcript (proper nouns, code \
             identifiers, library names); do NOT insert any that were not spoken: ",
        );
        pre.push_str(&ctx.nearby_terms.join(", "));
        pre.push('\n');
    }
    pre.push_str(
        "Apply the above as guidance over the cleanup rules below. Priority when \
         instructions conflict: the user's app instructions first, then the base \
         cleanup rules, then soft context. Keep edits minimal, preserve meaning, \
         and never invent content that was not dictated.\n\n",
    );
    pre.push_str(base);
    pre
}

/// Detect the foreground app/site. `None` on unsupported platforms or on any
/// failure (caller then falls back to BASE-only). Cheap: one Win32 round-trip for
/// the app; UI Automation is consulted only for browser URLs and — when
/// `read_nearby_terms` is set — the focused field's unique terms.
pub fn detect_active_context(read_nearby_terms: bool) -> Option<ActiveContext> {
    #[cfg(windows)]
    {
        windows_impl::detect(read_nearby_terms)
    }
    #[cfg(not(windows))]
    {
        let _ = read_nearby_terms;
        None
    }
}

/// Read the currently focused editable field's full text via UI Automation, for
/// the auto-dictionary watcher. `None` on unsupported platforms, password fields,
/// or any failure. Silent — no UI.
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
    use super::{category_for_exe, AppCategory, ActiveContext};
    use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    pub(super) fn detect(read_nearby_terms: bool) -> Option<ActiveContext> {
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }

            let exe_path = process_image_path(pid)?;
            // Stem = file name without extension, lowercased.
            let exe = std::path::Path::new(&exe_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if exe.is_empty() {
                return None;
            }

            let category = category_for_exe(&exe);
            let app_name = window_title(hwnd).unwrap_or_else(|| exe.clone());

            // UI Automation is only worth spinning up when we actually need it:
            // a browser (for the URL) or the nearby-terms opt-in. Everything here
            // is best-effort and SILENT — any failure just yields None/empty.
            let is_browser = category == AppCategory::Browser;
            let (url_host, nearby_terms) = if is_browser || read_nearby_terms {
                super::uia::read(hwnd, is_browser, read_nearby_terms)
            } else {
                (None, Vec::new())
            };

            Some(ActiveContext {
                app_name,
                exe,
                category,
                url_host,
                nearby_terms,
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
    use super::{extract_unique_terms, host_from_url};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
        TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_EditControlTypeId,
        UIA_ValuePatternId,
    };

    /// Upper bound on `Edit` controls inspected when hunting the address bar —
    /// keeps a page full of inputs from making URL detection expensive.
    const MAX_EDIT_SCAN: i32 = 60;

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

    /// Read the URL host (when `want_url`) and the focused-field unique terms
    /// (when `want_terms`). Returns `(None, vec![])` on any failure.
    pub(super) fn read(
        hwnd: HWND,
        want_url: bool,
        want_terms: bool,
    ) -> (Option<String>, Vec<String>) {
        unsafe {
            let _com = ComGuard::init();
            let automation: IUIAutomation =
                match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                    Ok(a) => a,
                    Err(_) => return (None, Vec::new()),
                };
            let url = if want_url { read_url(&automation, hwnd) } else { None };
            let terms = if want_terms {
                read_focused_terms(&automation)
            } else {
                Vec::new()
            };
            (url, terms)
        }
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
        let root = automation.ElementFromHandle(hwnd).ok()?;
        let cond = automation
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
            .ok()?;
        let edits = root.FindAll(TreeScope_Descendants, &cond).ok()?;
        let len = edits.Length().unwrap_or(0);
        // Cap the scan so a page with many inputs can't make this expensive.
        for i in 0..len.min(MAX_EDIT_SCAN) {
            let Ok(edit) = edits.GetElement(i) else {
                continue;
            };
            // The URL can surface as the edit's value (typical) or, on some
            // browsers, its name — try both.
            let candidate = read_value(&edit).or_else(|| read_name(&edit));
            if let Some(host) = candidate.and_then(|v| host_from_url(&v)) {
                return Some(host);
            }
        }
        None
    }

    /// Unique non-dictionary terms from the currently focused element's value.
    /// Password fields are skipped outright.
    unsafe fn read_focused_terms(automation: &IUIAutomation) -> Vec<String> {
        let Ok(el) = automation.GetFocusedElement() else {
            return Vec::new();
        };
        if is_password(&el) {
            return Vec::new();
        }
        match read_value(&el) {
            Some(text) => extract_unique_terms(&text),
            None => Vec::new(),
        }
    }

    /// The focused field's raw text (auto-dictionary watcher). Own COM scope so it
    /// is safe to call standalone from the watcher thread. Password fields skipped.
    pub(in crate::context_detect) fn read_focused_value() -> Option<String> {
        unsafe {
            let _com = ComGuard::init();
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let el = automation.GetFocusedElement().ok()?;
            if is_password(&el) {
                return None;
            }
            read_value(&el)
        }
    }

    /// The element's `ValuePattern` value as a non-empty `String`, if available.
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
            category,
            url_host: None,
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

    #[test]
    fn disabled_returns_base_untouched() {
        let s = AppSettings::default(); // context_awareness_enabled = false
        let base = "BASE PROMPT ${output}";
        assert_eq!(compose_prompt(base, &s, Some(&ctx("code", AppCategory::Ide))), base);
    }

    #[test]
    fn other_category_with_no_mode_adds_nothing() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let base = "BASE ${output}";
        assert_eq!(
            compose_prompt(base, &s, Some(&ctx("unknownapp", AppCategory::Other))),
            base
        );
    }

    #[test]
    fn soft_context_is_prepended_for_known_category() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        let base = "BASE ${output}";
        let out = compose_prompt(base, &s, Some(&ctx("code", AppCategory::Ide)));
        assert!(out.starts_with("[Context awareness]"));
        assert!(out.contains("code editor"));
        assert!(out.ends_with(base)); // base preserved verbatim at the tail.
    }

    #[test]
    fn process_mode_matches_and_injects_highest_priority() {
        let mut s = AppSettings::default();
        s.context_awareness_enabled = true;
        s.app_modes.push(AppMode {
            id: "x".into(),
            name: "X post".into(),
            matcher: AppMatch::Process("chrome".into()),
            prompt: "Rewrite as a tweet under 280 chars.".into(),
            enabled: true,
        });
        let out = compose_prompt("BASE ${output}", &s, Some(&ctx("chrome", AppCategory::Browser)));
        assert!(out.contains("HIGHEST priority"));
        assert!(out.contains("tweet under 280"));
    }

    #[test]
    fn url_host_suffix_match() {
        let m = AppMode {
            id: "g".into(),
            name: "Gmail".into(),
            matcher: AppMatch::UrlHost("mail.google.com".into()),
            prompt: "p".into(),
            enabled: true,
        };
        let mut c = ctx("chrome", AppCategory::Browser);
        c.url_host = Some("mail.google.com".into());
        assert!(mode_matches(&m, &c));
        c.url_host = Some("www.mail.google.com".into());
        assert!(mode_matches(&m, &c));
        c.url_host = Some("docs.google.com".into());
        assert!(!mode_matches(&m, &c));
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
        for w in ["asked", "the", "hook", "bug", "today", "because", "was", "broken"] {
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
        let out = compose_prompt("BASE ${output}", &s, Some(&c));
        assert!(out.contains("Nearby terms"));
        assert!(out.contains("Rita, PyTorch"));
    }

    #[cfg(windows)]
    #[test]
    fn host_from_url_parsing() {
        assert_eq!(host_from_url("https://mail.google.com/mail/u/0"), Some("mail.google.com".into()));
        assert_eq!(host_from_url("mail.google.com/mail"), Some("mail.google.com".into()));
        assert_eq!(host_from_url("https://www.x.com/home"), Some("x.com".into()));
        assert_eq!(host_from_url("user:pass@host.example.com:8080/p"), Some("host.example.com".into()));
        assert_eq!(host_from_url("how to parse a url"), None); // search query.
        assert_eq!(host_from_url(""), None);
        assert_eq!(host_from_url("localhost"), None); // no dot → not host-shaped.
    }
}
