//! [GRAIN] Reach: the Agent's first-party OS tools (`docs/Reach/PLAN.md`).
//!
//! The desktop half of `grain_core::reach`. That module decides what a call
//! means and whether it is allowed. This one carries it out with the OS's own
//! doors: the default handler for links (`tauri-plugin-opener`, never a shell),
//! the Start menu catalogue for apps, UI Automation to check what has focus,
//! Grain's existing paste path for text, and synthetic keys for a small fixed
//! set of keys. Nothing here moves the mouse or reads the screen's pixels.
//!
//! Lifetime: no thread, hook or cache outlives a call. Every call runs on a
//! blocking worker and drops what it opened (COM, the app catalogue) on return.
//!
//! Testing policy (user decision 2026-11-13): calls run without a confirmation
//! dialog. The hard refusals stay: dangerous links and file types (core),
//! password fields, Grain's own windows, and Enter or typing in terminals.

use std::sync::atomic::{AtomicIsize, Ordering};

use grain_core::capability_index::{ActionInput, ExtensionDirectoryEntry};
use grain_core::execution::{ActionOutcome, FailureClass, SuccessData};
use grain_core::reach::{self, Call, Key, Media};
use grain_sdk::action::ActionRisk;
use tauri::AppHandle;

pub(crate) use grain_core::reach::EXTENSION_ID;

/// The window that was in front when the Agent was summoned (0 = none). It is
/// where "type this" goes when Grain's own notch has focus.
static SUMMON_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Record the window the user summoned the Agent from. Called at every summon
/// and hand-open; a later summon overwrites it.
pub(crate) fn remember_summon_target(hwnd: Option<isize>) {
    SUMMON_TARGET.store(hwnd.unwrap_or(0), Ordering::SeqCst);
}

pub(crate) fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).reach_enabled
}

/// [GRAIN] Turn Reach on or off (Extensions page). Off removes its tools from
/// the Agent on the next model round; a running call finishes.
#[tauri::command]
#[specta::specta]
pub fn change_reach_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.reach_enabled = enabled;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Reach's Agent directory entry, when the user has turned it on.
pub(crate) fn directory(app: &AppHandle) -> Option<ExtensionDirectoryEntry> {
    is_enabled(app).then(|| ExtensionDirectoryEntry {
        extension_id: EXTENSION_ID.to_string(),
        name: reach::PROVIDER_NAME.to_string(),
        description: reach::DESCRIPTION.to_string(),
        action_count: reach::tools().len(),
    })
}

/// One catalogue row: the retrieval record plus the model-facing definition.
pub(crate) struct CatalogTool {
    pub action: ActionInput,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// The compiled tool set and its digest. No I/O.
pub(crate) fn catalog() -> (String, Vec<CatalogTool>) {
    let tools = reach::tools()
        .into_iter()
        .map(|tool| CatalogTool {
            action: ActionInput {
                extension_id: EXTENSION_ID.to_string(),
                action_id: tool.name.to_string(),
                canonical_id: format!("{EXTENSION_ID}:{}", tool.name),
                provider_name: reach::PROVIDER_NAME.to_string(),
                title: tool.title.to_string(),
                aliases: vec![reach::PROVIDER_NAME.to_string(), tool.name.to_string()],
                namespaces: Vec::new(),
                tags: tool.keywords.iter().map(|k| k.to_string()).collect(),
                examples: Vec::new(),
                phrases: grain_core::capability_agent::tool_search_phrases(tool.name),
                params: Vec::new(),
                when_to_use: String::new(),
                when_not_to_use: String::new(),
                description: format!(
                    "{} {}",
                    tool.description,
                    grain_core::capability_agent::schema_search_text(&tool.schema)
                ),
                provider_context: vec![reach::DESCRIPTION.to_string()],
                risk: if tool.read_only {
                    ActionRisk::Safe
                } else {
                    ActionRisk::Confirm
                },
                enabled: true,
                platform_ok: true,
                quarantined: false,
            },
            description: tool.description.to_string(),
            parameters: tool.schema,
        })
        .collect();
    (reach::catalog_digest(), tools)
}

/// Run one Reach action. Never panics into the Agent loop: every failure is a
/// typed outcome the model can read and correct.
pub(crate) async fn call(
    app: &AppHandle,
    action_id: &str,
    arguments: &serde_json::Value,
) -> ActionOutcome {
    if !is_enabled(app) {
        return failed(FailureClass::Cancelled, "Reach is turned off.");
    }
    let parsed = match reach::parse(action_id, arguments) {
        Ok(call) => call,
        Err(message) => return failed(FailureClass::InvalidArgument, &message),
    };
    let title = reach::tools()
        .into_iter()
        .find(|tool| tool.name == action_id)
        .map(|tool| tool.title.to_string());
    let read_only = matches!(parsed, Call::ListWindows);
    log::info!(
        "[GRAIN] reach: {action_id} {}",
        // Arguments can be private text; log the shape only.
        describe_for_log(&parsed)
    );
    let app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || run(&app, parsed))
        .await
        .unwrap_or_else(|error| Err(Failure::internal(format!("Reach stopped: {error}"))));
    match result {
        Ok(body) => ActionOutcome::Succeeded(SuccessData {
            source: Some(reach::PROVIDER_NAME.to_string()),
            title,
            body: Some(body),
            structured_content: None,
            details: Vec::new(),
            receipt: !read_only,
        }),
        Err(failure) => {
            log::info!("[GRAIN] reach: {action_id} failed: {}", failure.message);
            failed(failure.class, &failure.message)
        }
    }
}

fn describe_for_log(call: &Call) -> String {
    match call {
        Call::OpenLink { url, .. } => {
            format!("scheme={}", url.split(':').next().unwrap_or_default())
        }
        Call::OpenApp { .. } => "app".into(),
        Call::TypeText { text, replace } => {
            format!("chars={} replace={replace}", text.chars().count())
        }
        Call::PressKey(key) => key.label().into(),
        Call::Media(media) => format!("{media:?}"),
        Call::OpenSettings(page) => (*page).into(),
        Call::ShowFile { open, .. } => format!("open={open}"),
        Call::ListWindows => String::new(),
    }
}

fn failed(class: FailureClass, message: &str) -> ActionOutcome {
    ActionOutcome::Failed {
        class,
        message: message.to_string(),
    }
}

struct Failure {
    class: FailureClass,
    message: String,
}

impl Failure {
    fn new(class: FailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Internal, message)
    }
    fn refused(message: impl Into<String>) -> Self {
        Self::new(FailureClass::InvalidArgument, message)
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self::new(FailureClass::NotFound, message)
    }
    fn unsupported(what: &str) -> Self {
        Self::new(
            FailureClass::NotFound,
            format!("{what} is not available on this operating system yet."),
        )
    }
}

type Outcome = Result<String, Failure>;

fn run(app: &AppHandle, call: Call) -> Outcome {
    match call {
        Call::OpenLink {
            url,
            browser,
            label,
        } => open_link(app, &url, browser.as_deref(), &label),
        Call::OpenApp { name } => open_app(&name),
        Call::TypeText { text, replace } => type_text(app, text, replace),
        Call::PressKey(key) => press_key(app, key),
        Call::Media(media) => media_control(app, media),
        Call::OpenSettings(page) => open_settings(app, page),
        Call::ShowFile { path, open } => show_file(app, &path, open),
        Call::ListWindows => list_windows(),
    }
}

// ── Links ──────────────────────────────────────────────────────────────────

fn open_link(app: &AppHandle, url: &str, browser: Option<&str>, label: &str) -> Outcome {
    use tauri_plugin_opener::OpenerExt;
    let scheme = url
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let browser_path = match browser {
        Some(name) => Some(resolve_browser(name)?),
        None => None,
    };
    if !reach::is_web(url) {
        handler_installed(&scheme)?;
    }
    let before = os::foreground();
    app.opener()
        .open_url(url, browser_path.as_ref().map(|(_, path)| path.as_str()))
        .map_err(|error| Failure::internal(format!("Could not open the link: {error}")))?;
    os::await_new_foreground(before);
    let via = browser_path
        .map(|(name, _)| format!(" in {name}"))
        .unwrap_or_default();
    Ok(format!("Opened {label}{via}."))
}

/// A browser named by the model, resolved to an executable from the Start menu.
fn resolve_browser(name: &str) -> Result<(String, String), Failure> {
    let apps = crate::context_detect::app_catalog::installed_apps();
    let index = reach::best_app_match(name, apps.iter().map(|app| app.name.as_str()))
        .ok_or_else(|| Failure::not_found(format!("No installed browser matches \"{name}\".")))?;
    let found = &apps[index];
    if !found.icon_id.to_ascii_lowercase().ends_with(".exe") {
        return Err(Failure::not_found(format!(
            "{} cannot be chosen as the browser; omit `browser` to use the default.",
            found.name
        )));
    }
    Ok((found.name.clone(), found.icon_id.clone()))
}

/// Refuse an app link nobody can open, instead of letting the OS show a
/// "you'll need a new app" dialog.
fn handler_installed(scheme: &str) -> Result<(), Failure> {
    if os::scheme_has_handler(scheme) {
        Ok(())
    } else {
        Err(Failure::not_found(format!(
            "No app on this computer opens `{scheme}:` links. Try open_app, or the web version via search_site/open_link."
        )))
    }
}

fn open_settings(app: &AppHandle, page: &'static str) -> Outcome {
    if !cfg!(windows) {
        return Err(Failure::unsupported("Opening a settings page"));
    }
    open_link(
        app,
        reach::windows_settings_uri(page),
        None,
        &format!("{} settings", page.replace('_', " ")),
    )
}

// ── Files ──────────────────────────────────────────────────────────────────

fn show_file(app: &AppHandle, raw: &str, open: bool) -> Outcome {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let mut parts = raw.splitn(2, ['\\', '/']);
    let first = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default();
    let paths = app.path();
    let base = match first.to_ascii_lowercase().as_str() {
        "~" => paths.home_dir().ok(),
        "downloads" => paths.download_dir().ok(),
        "documents" => paths.document_dir().ok(),
        "desktop" => paths.desktop_dir().ok(),
        "pictures" => paths.picture_dir().ok(),
        "music" => paths.audio_dir().ok(),
        "videos" => paths.video_dir().ok(),
        _ => None,
    };
    let path = match base {
        Some(base) if rest.is_empty() => base,
        Some(base) => base.join(rest),
        None => std::path::PathBuf::from(raw),
    };
    let metadata = std::fs::metadata(&path)
        .map_err(|_| Failure::not_found(format!("{} does not exist.", path.display())))?;
    let shown = path.display().to_string();
    if !open {
        app.opener()
            .reveal_item_in_dir(&path)
            .map_err(|error| Failure::internal(format!("Could not show the file: {error}")))?;
        return Ok(format!("Showed {shown} in its folder."));
    }
    if !metadata.is_dir() {
        // The core list checked the name; the OS knows types we do not.
        if reach::is_dangerous_file(&shown) || os::is_dangerous_type(&path) {
            return Err(Failure::refused(
                "Reach never opens programs, scripts or installers. Use open=false to show it in its folder instead.",
            ));
        }
    }
    app.opener()
        .open_path(shown.clone(), None::<&str>)
        .map_err(|error| Failure::internal(format!("Could not open it: {error}")))?;
    Ok(format!("Opened {shown}."))
}

// ── Apps, text, keys, media, windows ───────────────────────────────────────

fn open_app(name: &str) -> Outcome {
    if !cfg!(windows) {
        return Err(Failure::unsupported("Opening apps by name"));
    }
    let apps = crate::context_detect::app_catalog::installed_apps();
    let index =
        reach::best_app_match(name, apps.iter().map(|app| app.name.as_str())).ok_or_else(|| {
            Failure::not_found(format!(
                "No installed app matches \"{name}\". Check the name, or open its website instead."
            ))
        })?;
    let found = &apps[index];
    if let Some(window) = os::find_app_window(&found.target) {
        if os::bring_to_front(window) {
            return Ok(format!("Switched to {}.", found.name));
        }
    }
    let before = os::foreground();
    os::launch(&found.icon_id)
        .map_err(|error| Failure::internal(format!("Could not open {}: {error}", found.name)))?;
    if os::await_new_foreground(before) {
        Ok(format!("Opened {}.", found.name))
    } else {
        Ok(format!(
            "Started {}; its window is not in front yet (it may still be loading).",
            found.name
        ))
    }
}

/// The window Reach types into: whatever app is in front, or, when Grain's own
/// notch has focus, the window the Agent was summoned from.
fn target_window() -> Result<os::Window, Failure> {
    if let Some(window) = os::foreground().filter(|w| !os::is_own_window(*w)) {
        return Ok(window);
    }
    let summoned = SUMMON_TARGET.load(Ordering::SeqCst);
    if summoned != 0 && os::is_window(summoned) && !os::is_own_window(summoned) {
        if os::bring_to_front(summoned) {
            return Ok(summoned);
        }
        return Err(Failure::internal(
            "Could not bring the app you were using back to the front.",
        ));
    }
    Err(Failure::not_found(
        "There is no app window to type into. Open the app first, or summon the Agent from it.",
    ))
}

fn type_text(app: &AppHandle, text: String, replace: bool) -> Outcome {
    if !cfg!(windows) {
        return Err(Failure::unsupported("Typing into apps"));
    }
    let window = target_window()?;
    let app_name = os::window_label(window);
    if crate::agent::foreground_is_terminal() {
        return Err(Failure::refused(format!(
            "Reach does not type into terminals ({app_name}); a command line runs what it receives."
        )));
    }
    let probe = crate::context_detect::read_focus_probe();
    let target = match &probe {
        Some(probe) => {
            if probe.facts.is_password {
                return Err(Failure::refused("The focused field is a password field."));
            }
            if probe.facts.is_own_process {
                return Err(Failure::refused("Focus is in Grain itself."));
            }
            crate::context_detect::classify(probe.facts)
        }
        None => crate::context_detect::FocusTarget::Unknown,
    };
    match target {
        crate::context_detect::FocusTarget::NotEditable => {
            return Err(Failure::not_found(format!(
                "The focused element in {app_name} is not a text field. Use press_key tab to move to a field, or ask the user to click into it."
            )))
        }
        crate::context_detect::FocusTarget::Unknown if replace => {
            return Err(Failure::not_found(format!(
                "Could not confirm a text field has focus in {app_name}, so its content was not replaced. Try mode=insert."
            )))
        }
        _ => {}
    }
    if replace {
        os::select_all(app)?;
    }
    let chars = text.chars().count();
    crate::utils::paste_with_options(text, app.clone(), true)
        .map_err(|error| Failure::internal(format!("Could not type the text: {error}")))?;
    Ok(format!(
        "{} {chars} characters in {app_name}.",
        if replace {
            "Replaced the field with"
        } else {
            "Typed"
        }
    ))
}

fn press_key(app: &AppHandle, key: Key) -> Outcome {
    if !cfg!(windows) {
        return Err(Failure::unsupported("Pressing keys"));
    }
    let window = target_window()?;
    let app_name = os::window_label(window);
    let submits = matches!(key, Key::Enter | Key::CtrlEnter | Key::ShiftEnter);
    if submits && crate::agent::foreground_is_terminal() {
        return Err(Failure::refused(format!(
            "Reach does not press Enter in terminals ({app_name})."
        )));
    }
    os::press(app, key)?;
    Ok(format!("Pressed {} in {app_name}.", key.label()))
}

fn media_control(app: &AppHandle, media: Media) -> Outcome {
    if !cfg!(windows) {
        return Err(Failure::unsupported("Media and volume control"));
    }
    os::media(app, media)
}

fn list_windows() -> Outcome {
    if !cfg!(windows) {
        return Err(Failure::unsupported("Listing windows"));
    }
    let mut out = String::new();
    let focus = os::foreground().filter(|w| !os::is_own_window(*w));
    match focus {
        Some(window) => {
            let probe = crate::context_detect::read_focus_probe();
            let field = match probe
                .as_ref()
                .map(|p| (p.facts, crate::context_detect::classify(p.facts)))
            {
                Some((facts, _)) if facts.is_password => {
                    "a password field (Reach will not type there)"
                }
                Some((_, crate::context_detect::FocusTarget::Editable)) => "an editable text field",
                Some((_, crate::context_detect::FocusTarget::NotEditable)) => "not a text field",
                _ => "unknown (may or may not accept text)",
            };
            out.push_str(&format!(
                "In front: {}. Keyboard focus: {field}.\n",
                os::window_label(window)
            ));
        }
        None => out.push_str("In front: Grain (the Agent).\n"),
    }
    let windows = os::list_windows();
    out.push_str(&format!("Open windows ({}):\n", windows.len()));
    for (app_name, title) in windows {
        out.push_str(&format!("- {app_name}: {title}\n"));
    }
    Ok(out)
}

// ── Windows adapter ────────────────────────────────────────────────────────

#[cfg(windows)]
mod os {
    use super::{Failure, Key, Media, Outcome};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Manager};
    use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Shell::{
        AssocIsDangerous, AssocQueryStringW, ShellExecuteW, ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
        IsWindowVisible, PeekMessageW, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, MSG,
        PM_NOREMOVE, SW_RESTORE, SW_SHOWNORMAL, WS_EX_TOOLWINDOW,
    };

    pub(super) type Window = isize;

    fn hwnd(window: Window) -> HWND {
        HWND(window as *mut _)
    }

    pub(super) fn foreground() -> Option<Window> {
        let window = unsafe { GetForegroundWindow() };
        (!window.0.is_null()).then_some(window.0 as isize)
    }

    pub(super) fn is_window(window: Window) -> bool {
        unsafe { IsWindow(Some(hwnd(window))).as_bool() }
    }

    fn window_pid(window: Window) -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd(window), Some(&mut pid)) };
        pid
    }

    pub(super) fn is_own_window(window: Window) -> bool {
        window_pid(window) == unsafe { GetCurrentProcessId() }
    }

    /// Wait (bounded) for a new window to come to the front after a launch or
    /// link, so a following `type_text` lands in it. True when one did.
    pub(super) fn await_new_foreground(before: Option<Window>) -> bool {
        let deadline = Instant::now() + Duration::from_secs(6);
        while Instant::now() < deadline {
            if let Some(now) = foreground() {
                if Some(now) != before && !is_own_window(now) {
                    // Give the new window a moment to place its own focus.
                    std::thread::sleep(Duration::from_millis(250));
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    fn title(window: Window) -> String {
        unsafe {
            let len = GetWindowTextLengthW(hwnd(window));
            if len <= 0 {
                return String::new();
            }
            let mut buf = vec![0u16; len as usize + 1];
            let read = GetWindowTextW(hwnd(window), &mut buf);
            String::from_utf16_lossy(&buf[..read.max(0) as usize])
        }
    }

    /// Image path and packaged identity of the process owning `window`.
    fn process_identity(window: Window) -> Option<(String, Option<String>)> {
        let pid = window_pid(window);
        if pid == 0 {
            return None;
        }
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; MAX_PATH as usize];
            let mut len = buf.len() as u32;
            let path = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .ok()
            .map(|_| String::from_utf16_lossy(&buf[..len as usize]));
            let mut aumid_len = 512u32;
            let mut aumid = vec![0u16; aumid_len as usize];
            let aumid =
                GetApplicationUserModelId(handle, &mut aumid_len, Some(PWSTR(aumid.as_mut_ptr())))
                    .is_ok()
                    .then(|| {
                        let end = (aumid_len as usize).min(aumid.len()).saturating_sub(1);
                        String::from_utf16_lossy(&aumid[..end])
                    })
                    .filter(|s| !s.is_empty());
            let _ = CloseHandle(handle);
            Some((path?, aumid))
        }
    }

    fn exe_stem(path: &str) -> String {
        std::path::Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    /// "App: title" for result messages.
    pub(super) fn window_label(window: Window) -> String {
        let app = process_identity(window)
            .map(|(path, _)| exe_stem(&path))
            .unwrap_or_default();
        let title = title(window);
        match (app.is_empty(), title.trim().is_empty()) {
            (false, false) => format!("{app} (\"{}\")", super::clip_title(&title)),
            (false, true) => app,
            (true, false) => format!("\"{}\"", super::clip_title(&title)),
            (true, true) => "the app in front".into(),
        }
    }

    /// Top-level windows a person would call "open": visible, titled, not
    /// tool windows, not owned popups, not cloaked (hidden UWP frames).
    fn app_windows() -> Vec<Window> {
        unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> BOOL {
            let out = &mut *(data.0 as *mut Vec<Window>);
            if out.len() < 256 {
                out.push(window.0 as isize);
            }
            BOOL(1)
        }
        let mut all: Vec<Window> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
        }
        all.into_iter()
            .filter(|&window| unsafe {
                let h = hwnd(window);
                if !IsWindowVisible(h).as_bool() || is_own_window(window) {
                    return false;
                }
                if GetWindow(h, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
                    return false;
                }
                if (GetWindowLongW(h, GWL_EXSTYLE) as u32) & WS_EX_TOOLWINDOW.0 != 0 {
                    return false;
                }
                let mut cloaked = 0u32;
                if DwmGetWindowAttribute(
                    h,
                    DWMWA_CLOAKED,
                    &mut cloaked as *mut _ as *mut _,
                    std::mem::size_of::<u32>() as u32,
                )
                .is_ok()
                    && cloaked != 0
                {
                    return false;
                }
                GetWindowTextLengthW(h) > 0
            })
            .collect()
    }

    pub(super) fn list_windows() -> Vec<(String, String)> {
        app_windows()
            .into_iter()
            .take(40)
            .map(|window| {
                let app = process_identity(window)
                    .map(|(path, _)| exe_stem(&path))
                    .unwrap_or_else(|| "?".into());
                (app, super::clip_title(&title(window)))
            })
            .collect()
    }

    /// A visible window of the catalogue app `target` (exe stem or AUMID).
    pub(super) fn find_app_window(target: &str) -> Option<Window> {
        let packaged = target.contains('!');
        app_windows().into_iter().find(|&window| {
            process_identity(window).is_some_and(|(path, aumid)| {
                if packaged {
                    aumid
                        .as_deref()
                        .is_some_and(|id| id.eq_ignore_ascii_case(target))
                } else {
                    exe_stem(&path) == target
                }
            })
        })
    }

    /// Bring `window` to the front. Windows only lets the foreground process
    /// move focus; when Grain is not it, attach to the foreground thread's
    /// input for the one call, the documented exception.
    pub(super) fn bring_to_front(window: Window) -> bool {
        let h = hwnd(window);
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            if SetForegroundWindow(h).as_bool() && foreground() == Some(window) {
                return true;
            }
            if let Some(current) = foreground() {
                let theirs = GetWindowThreadProcessId(hwnd(current), None);
                let ours = GetCurrentThreadId();
                // AttachThreadInput needs this thread to own a message queue.
                let mut msg = MSG::default();
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                use windows::Win32::System::Threading::AttachThreadInput;
                if theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, true).as_bool()
                {
                    let _ = BringWindowToTop(h);
                    let _ = SetForegroundWindow(h);
                    let _ = AttachThreadInput(ours, theirs, false);
                }
            }
        }
        let deadline = Instant::now() + Duration::from_millis(600);
        while Instant::now() < deadline {
            if foreground() == Some(window) {
                std::thread::sleep(Duration::from_millis(120));
                return true;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        false
    }

    /// Launch a catalogue app by its executable path or AppUserModelID, with
    /// no arguments.
    pub(super) fn launch(icon_id: &str) -> Result<(), String> {
        let file = if icon_id.contains('!') && !icon_id.contains('\\') {
            format!("shell:AppsFolder\\{icon_id}")
        } else {
            icon_id.to_string()
        };
        let file = HSTRING::from(file);
        let result = unsafe {
            ShellExecuteW(
                None,
                &HSTRING::from("open"),
                &file,
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute reports success as a value greater than 32.
        if result.0 as isize > 32 {
            Ok(())
        } else {
            Err(format!(
                "Windows refused the launch (code {})",
                result.0 as isize
            ))
        }
    }

    pub(super) fn scheme_has_handler(scheme: &str) -> bool {
        let mut len = 0u32;
        let scheme = HSTRING::from(scheme);
        // Asking for the length is enough: it fails when nothing is registered.
        unsafe {
            AssocQueryStringW(
                ASSOCF_IS_PROTOCOL,
                ASSOCSTR_EXECUTABLE,
                &scheme,
                PCWSTR::null(),
                None,
                &mut len,
            )
            .is_ok()
                && len > 1
        }
    }

    pub(super) fn is_dangerous_type(path: &std::path::Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        unsafe { AssocIsDangerous(&HSTRING::from(format!(".{ext}"))).as_bool() }
    }

    fn with_enigo<T>(
        app: &AppHandle,
        f: impl FnOnce(&mut enigo::Enigo) -> Result<T, String>,
    ) -> Result<T, Failure> {
        let state = app
            .try_state::<crate::input::EnigoState>()
            .ok_or_else(|| Failure::internal("The input controller is unavailable."))?;
        let mut enigo = state
            .0
            .lock()
            .map_err(|_| Failure::internal("The input controller is busy."))?;
        crate::input::release_modifiers(&mut enigo);
        f(&mut enigo).map_err(Failure::internal)
    }

    fn tap(
        enigo: &mut enigo::Enigo,
        modifier: Option<enigo::Key>,
        key: enigo::Key,
    ) -> Result<(), String> {
        use enigo::{Direction, Keyboard};
        if let Some(modifier) = modifier {
            enigo
                .key(modifier, Direction::Press)
                .map_err(|e| e.to_string())?;
        }
        let pressed = enigo.key(key, Direction::Click).map_err(|e| e.to_string());
        if let Some(modifier) = modifier {
            let _ = enigo.key(modifier, Direction::Release);
        }
        pressed
    }

    pub(super) fn select_all(app: &AppHandle) -> Result<(), Failure> {
        with_enigo(app, |enigo| {
            tap(enigo, Some(enigo::Key::Control), enigo::Key::Unicode('a'))
        })?;
        std::thread::sleep(Duration::from_millis(60));
        Ok(())
    }

    pub(super) fn press(app: &AppHandle, key: Key) -> Result<(), Failure> {
        use enigo::Key as K;
        with_enigo(app, |enigo| match key {
            Key::Enter => {
                crate::clipboard::send_return_key(enigo, crate::settings::AutoSubmitKey::Enter)
            }
            Key::CtrlEnter => {
                crate::clipboard::send_return_key(enigo, crate::settings::AutoSubmitKey::CtrlEnter)
            }
            Key::ShiftEnter => tap(enigo, Some(K::Shift), K::Return),
            Key::Tab => tap(enigo, None, K::Tab),
            Key::ShiftTab => tap(enigo, Some(K::Shift), K::Tab),
            Key::Escape => tap(enigo, None, K::Escape),
        })
    }

    pub(super) fn media(app: &AppHandle, media: Media) -> Outcome {
        use enigo::Key as K;
        let key = match media {
            Media::PlayPause => Some((K::MediaPlayPause, "Toggled play/pause.")),
            Media::Next => Some((K::MediaNextTrack, "Skipped to the next track.")),
            Media::Previous => Some((K::MediaPrevTrack, "Went to the previous track.")),
            Media::Stop => Some((K::MediaStop, "Stopped playback.")),
            _ => None,
        };
        if let Some((key, message)) = key {
            with_enigo(app, |enigo| tap(enigo, None, key))?;
            return Ok(message.into());
        }
        volume(media)
    }

    /// System volume through Core Audio: exact levels, no key-repeat guessing.
    fn volume(media: Media) -> Outcome {
        use windows::Win32::Media::Audio::{
            eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
            MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
        };
        let failure =
            |e: windows::core::Error| Failure::internal(format!("Volume control failed: {e}"));
        unsafe {
            let co = CoInitializeEx(None, COINIT_MULTITHREADED);
            let result = (|| -> Outcome {
                let devices: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(failure)?;
                let device = devices
                    .GetDefaultAudioEndpoint(eRender, eMultimedia)
                    .map_err(failure)?;
                let endpoint: IAudioEndpointVolume =
                    device.Activate(CLSCTX_ALL, None).map_err(failure)?;
                let current = endpoint.GetMasterVolumeLevelScalar().map_err(failure)?;
                let set = |level: f32| -> Result<u32, Failure> {
                    let level = level.clamp(0.0, 1.0);
                    endpoint
                        .SetMasterVolumeLevelScalar(level, std::ptr::null())
                        .map_err(failure)?;
                    if level > 0.0 {
                        let _ = endpoint.SetMute(false, std::ptr::null());
                    }
                    Ok((level * 100.0).round() as u32)
                };
                match media {
                    Media::VolumeUp => Ok(format!("Volume is now {}%.", set(current + 0.1)?)),
                    Media::VolumeDown => Ok(format!("Volume is now {}%.", set(current - 0.1)?)),
                    Media::SetVolume(level) => {
                        Ok(format!("Volume is now {}%.", set(level as f32 / 100.0)?))
                    }
                    Media::Mute => {
                        let muted = endpoint.GetMute().map_err(failure)?.as_bool();
                        endpoint
                            .SetMute(!muted, std::ptr::null())
                            .map_err(failure)?;
                        Ok(if muted { "Unmuted." } else { "Muted." }.into())
                    }
                    _ => Err(Failure::internal("Unknown media action.")),
                }
            })();
            if co.is_ok() {
                CoUninitialize();
            }
            result
        }
    }
}

// ── Other platforms (links and files work through the opener) ──────────────

#[cfg(not(windows))]
mod os {
    use super::{Failure, Key, Media, Outcome};
    use tauri::AppHandle;

    pub(super) type Window = isize;

    pub(super) fn foreground() -> Option<Window> {
        None
    }
    pub(super) fn is_window(_: Window) -> bool {
        false
    }
    pub(super) fn is_own_window(_: Window) -> bool {
        false
    }
    pub(super) fn await_new_foreground(_: Option<Window>) -> bool {
        false
    }
    pub(super) fn window_label(_: Window) -> String {
        "the app in front".into()
    }
    pub(super) fn list_windows() -> Vec<(String, String)> {
        Vec::new()
    }
    pub(super) fn find_app_window(_: &str) -> Option<Window> {
        None
    }
    pub(super) fn bring_to_front(_: Window) -> bool {
        false
    }
    pub(super) fn launch(_: &str) -> Result<(), String> {
        Err("not available on this operating system yet".into())
    }
    /// The OS reports a missing handler itself; let it.
    pub(super) fn scheme_has_handler(_: &str) -> bool {
        true
    }
    pub(super) fn is_dangerous_type(_: &std::path::Path) -> bool {
        false
    }
    pub(super) fn select_all(_: &AppHandle) -> Result<(), Failure> {
        Err(Failure::unsupported("Typing into apps"))
    }
    pub(super) fn press(_: &AppHandle, _: Key) -> Result<(), Failure> {
        Err(Failure::unsupported("Pressing keys"))
    }
    pub(super) fn media(_: &AppHandle, _: Media) -> Outcome {
        Err(Failure::unsupported("Media and volume control"))
    }
}

fn clip_title(title: &str) -> String {
    let title = title.trim();
    if title.chars().count() <= 80 {
        title.to_string()
    } else {
        let mut out: String = title.chars().take(80).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_rows_are_reach_owned_and_match_the_core_digest() {
        let (digest, tools) = catalog();
        assert_eq!(digest, reach::catalog_digest());
        assert_eq!(tools.len(), reach::tools().len());
        for tool in &tools {
            assert_eq!(tool.action.extension_id, EXTENSION_ID);
            assert_eq!(
                tool.action.canonical_id,
                format!("{EXTENSION_ID}:{}", tool.action.action_id)
            );
            assert_eq!(tool.parameters["type"], "object");
        }
        // Only the read-only tool is classed safe; the rest act on the computer.
        let safe: Vec<_> = tools
            .iter()
            .filter(|tool| tool.action.risk.is_safe())
            .map(|tool| tool.action.action_id.as_str())
            .collect();
        assert_eq!(safe, ["list_windows"]);
    }

    #[test]
    fn summon_target_is_overwritten_and_cleared() {
        remember_summon_target(Some(42));
        assert_eq!(SUMMON_TARGET.load(Ordering::SeqCst), 42);
        remember_summon_target(None);
        assert_eq!(SUMMON_TARGET.load(Ordering::SeqCst), 0);
    }
}
