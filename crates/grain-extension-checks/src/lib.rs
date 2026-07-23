//! Shared extension-project checks used by `grain-ext doctor` and registry CI.
//!
//! The checker is filesystem-only and returns deterministic findings instead of
//! printing or exiting. CI and the local CLI therefore consume identical code
//! and can choose their own presentation without duplicating policy.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use grain_sdk::{
    daemon_event_capability, ExtensionProjectManifest, GrainPack, PackPayloads, Tier,
    DAEMON_EVENT_VARIANTS, GRAIN_API_VERSION,
};

pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_PROJECT_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = MAX_PROJECT_FILE_BYTES;

const MAX_OVERLAY_WIDTH: u32 = 720;
const MAX_OVERLAY_HEIGHT: u32 = 480;
const MAX_OVERLAY_TIMEOUT_MS: u32 = 15_000;

const IGNORED_DIRECTORIES: &[&str] = &[".git", "dist", "node_modules", "target"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl Finding {
    fn project(code: &'static str, path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            line: None,
            column: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", display_path(&self.path))?;
        if let Some(line) = self.line {
            write!(formatter, ":{line}")?;
            if let Some(column) = self.column {
                write!(formatter, ":{column}")?;
            }
        }
        write!(formatter, " [{}] {}", self.code, self.message)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoctorReport {
    pub findings: Vec<Finding>,
    pub files_checked: usize,
}

impl DoctorReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

impl fmt::Display for DoctorReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_clean() {
            return write!(
                formatter,
                "doctor: 0 findings ({} files checked)",
                self.files_checked
            );
        }
        writeln!(
            formatter,
            "doctor: {} finding(s) ({} files checked)",
            self.findings.len(),
            self.files_checked
        )?;
        for finding in &self.findings {
            writeln!(formatter, "  {finding}")?;
        }
        Ok(())
    }
}

/// Run the complete local/CI project check suite.
pub fn doctor(root: &Path) -> DoctorReport {
    let mut report = DoctorReport::default();
    let root = match root.canonicalize() {
        Ok(root) if root.is_dir() => root,
        Ok(_) => {
            report.findings.push(Finding::project(
                "E_PROJECT",
                ".",
                "project root is not a directory",
            ));
            return report;
        }
        Err(error) => {
            report.findings.push(Finding::project(
                "E_PROJECT",
                ".",
                format!("open project root: {error}"),
            ));
            return report;
        }
    };

    scan_submitted_files(&root, &mut report);
    check_manifest(&root, &mut report);
    sort_findings(&mut report.findings);
    report
}

fn check_manifest(root: &Path, report: &mut DoctorReport) {
    let manifest_path = root.join("manifest.json");
    let relative = PathBuf::from("manifest.json");
    let metadata = match fs::metadata(&manifest_path) {
        Ok(metadata) => metadata,
        Err(error) => {
            report.findings.push(Finding::project(
                "E_MANIFEST",
                relative,
                format!("read manifest: {error}"),
            ));
            return;
        }
    };
    if metadata.len() > MAX_MANIFEST_BYTES {
        report.findings.push(Finding::project(
            "E_SIZE",
            relative,
            "manifest.json is larger than 1 MB",
        ));
        return;
    }

    let raw = match fs::read_to_string(&manifest_path) {
        Ok(raw) => raw,
        Err(error) => {
            report.findings.push(Finding::project(
                "E_MANIFEST",
                relative,
                format!("read manifest as UTF-8: {error}"),
            ));
            return;
        }
    };
    let project: ExtensionProjectManifest = match serde_json::from_str(&raw) {
        Ok(project) => project,
        Err(error) => {
            report.findings.push(Finding::project(
                "E_MANIFEST",
                relative,
                format!("parse manifest JSON: {error}"),
            ));
            return;
        }
    };

    check_entry(root, &project, report);
    check_api_version(&project, report);
    check_pack_contract(&project, report);
    check_activations(&project, report);
    check_surface_budgets(&project, report);
}

fn check_entry(root: &Path, project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    // A project entry is JavaScript for the scripted runtime only. Data packs
    // are inert and native companions select their platform binary from the
    // manifest, so neither has an `entry` file to inspect.
    if project.manifest.tier != Tier::Scripted {
        return;
    }

    let entry = Path::new(&project.entry);
    if entry.as_os_str().is_empty()
        || entry.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        report.findings.push(Finding::project(
            "E_ENTRY",
            "manifest.json",
            "entry must be a project-relative file",
        ));
        return;
    }

    let entry_path = root.join(entry);
    let Ok(metadata) = fs::metadata(&entry_path) else {
        // A freshly scaffolded project has not been built yet. CI checks the
        // same path after its build stage, while `doctor` remains useful before
        // the author's first `npm install`.
        return;
    };
    let canonical_entry = match entry_path.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            report.findings.push(Finding::project(
                "E_ENTRY",
                PathBuf::from(&project.entry),
                format!("resolve entry: {error}"),
            ));
            return;
        }
    };
    if !canonical_entry.starts_with(root) {
        report.findings.push(Finding::project(
            "E_ENTRY",
            PathBuf::from(&project.entry),
            "entry resolves outside the project",
        ));
        return;
    }
    if !metadata.is_file() {
        report.findings.push(Finding::project(
            "E_ENTRY",
            PathBuf::from(&project.entry),
            "entry is not a file",
        ));
        return;
    }
    if metadata.len() > MAX_ENTRY_BYTES {
        report.findings.push(Finding::project(
            "E_SIZE",
            PathBuf::from(&project.entry),
            "extension entry is larger than 5 MB",
        ));
    }
}

fn check_api_version(project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    if !api_requirement_supported(&project.manifest.grain_api, GRAIN_API_VERSION) {
        report.findings.push(Finding::project(
            "E_API_VERSION",
            "manifest.json",
            format!(
                "grainApi '{}' is not supported by SDK '{}'",
                project.manifest.grain_api, GRAIN_API_VERSION
            ),
        ));
    }
}

fn check_pack_contract(project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    let mut manifest = project.manifest.clone();
    if manifest.tier == Tier::Scripted && manifest.entry_source.trim().is_empty() {
        manifest.entry_source = "// checked from project entry".into();
    }
    let pack = GrainPack {
        manifest,
        payloads: PackPayloads::default(),
    };
    // `doctor` is a developer-project check. Native companions are permitted
    // only through this unpacked/developer validation boundary; installation
    // and import still use `validate()` and reject native code.
    let validation = if pack.manifest.tier == Tier::Native {
        pack.validate_dev()
    } else {
        pack.validate()
    };
    if let Err(error) = validation {
        report
            .findings
            .push(Finding::project("E_MANIFEST", "manifest.json", error));
    }
}

fn check_activations(project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    if !matches!(project.manifest.tier, Tier::Scripted | Tier::Native) {
        return;
    }
    // A declared session mode is itself an activation path: the host registers
    // its contributed shortcut and wakes the owner for the slow stage without
    // an `activation` array entry (Phase 4).
    if project.manifest.activation.is_empty() && project.manifest.contributes.session_mode.is_none()
    {
        report.findings.push(Finding::project(
            "E_ACTIVATION",
            "manifest.json",
            "runtime extensions require at least one activation or a sessionMode",
        ));
        return;
    }

    let shortcut_ids = project
        .manifest
        .contributes
        .shortcuts
        .iter()
        .map(|shortcut| shortcut.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for activation in &project.manifest.activation {
        if !seen.insert(activation.as_str()) {
            report.findings.push(Finding::project(
                "E_ACTIVATION",
                "manifest.json",
                format!("duplicate activation '{activation}'"),
            ));
            continue;
        }
        match activation.as_str() {
            "onTransform" => {
                require_activation_capability(project, activation, "transform:transcript", report)
            }
            "onStartup" => {}
            value if value.starts_with("onEvent:") => {
                let event = &value["onEvent:".len()..];
                if !DAEMON_EVENT_VARIANTS.contains(&event) {
                    report.findings.push(Finding::project(
                        "E_ACTIVATION",
                        "manifest.json",
                        format!("unknown daemon event '{event}'"),
                    ));
                } else if let Some(capability) = daemon_event_capability(event) {
                    require_activation_capability(project, activation, capability, report);
                }
            }
            value if value.starts_with("onShortcut:") => {
                let shortcut = &value["onShortcut:".len()..];
                if shortcut.is_empty() || !shortcut_ids.contains(shortcut) {
                    report.findings.push(Finding::project(
                        "E_ACTIVATION",
                        "manifest.json",
                        format!("activation '{value}' has no matching contributed shortcut"),
                    ));
                }
            }
            _ => report.findings.push(Finding::project(
                "E_ACTIVATION",
                "manifest.json",
                format!("unsupported activation '{activation}'"),
            )),
        }
    }
}

fn require_activation_capability(
    project: &ExtensionProjectManifest,
    activation: &str,
    capability: &str,
    report: &mut DoctorReport,
) {
    if !project
        .manifest
        .permissions
        .iter()
        .any(|permission| permission == capability)
    {
        report.findings.push(Finding::project(
            "E_CAPABILITY",
            "manifest.json",
            format!("activation '{activation}' requires permission '{capability}'"),
        ));
    }
}

fn check_surface_budgets(project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    let Some(overlay) = &project.manifest.surfaces.overlay else {
        return;
    };
    if let Some([width, height]) = overlay.size {
        if width == 0 || height == 0 || width > MAX_OVERLAY_WIDTH || height > MAX_OVERLAY_HEIGHT {
            report.findings.push(Finding::project(
                "E_BUDGET",
                "manifest.json",
                format!("overlay size {width}x{height} exceeds the 720x480 host budget or is zero"),
            ));
        }
    }
    if let Some(timeout) = overlay.timeout_ms {
        if timeout == 0 || timeout > MAX_OVERLAY_TIMEOUT_MS {
            report.findings.push(Finding::project(
                "E_BUDGET",
                "manifest.json",
                format!("overlay timeout {timeout} ms exceeds the 15000 ms host budget or is zero"),
            ));
        }
    }
}

fn scan_submitted_files(root: &Path, report: &mut DoctorReport) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                report.findings.push(Finding::project(
                    "E_IO",
                    relative_path(root, &directory),
                    format!("read directory: {error}"),
                ));
                continue;
            }
        };
        let mut collected = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => collected.push(entry),
                Err(error) => report.findings.push(Finding::project(
                    "E_IO",
                    relative_path(root, &directory),
                    format!("read directory entry: {error}"),
                )),
            }
        }
        let mut entries = collected;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    report.findings.push(Finding::project(
                        "E_IO",
                        relative_path(root, &path),
                        format!("inspect path: {error}"),
                    ));
                    continue;
                }
            };
            if file_type.is_symlink() {
                report.findings.push(Finding::project(
                    "E_SYMLINK",
                    relative_path(root, &path),
                    "submitted projects must not contain symbolic links",
                ));
                continue;
            }
            if file_type.is_dir() {
                if !is_ignored_directory(&path) {
                    pending.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            report.files_checked += 1;
            let relative = relative_path(root, &path);
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    report.findings.push(Finding::project(
                        "E_IO",
                        relative,
                        format!("inspect file: {error}"),
                    ));
                    continue;
                }
            };
            if metadata.len() > MAX_PROJECT_FILE_BYTES {
                report.findings.push(Finding::project(
                    "E_SIZE",
                    relative,
                    "submitted file is larger than 5 MB",
                ));
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    report.findings.push(Finding::project(
                        "E_IO",
                        relative,
                        format!("read file: {error}"),
                    ));
                    continue;
                }
            };
            if bytes.contains(&0) {
                continue;
            }
            if let Ok(text) = std::str::from_utf8(&bytes) {
                scan_unicode(&relative, text, &mut report.findings);
            }
        }
    }
}

fn scan_unicode(path: &Path, text: &str, findings: &mut Vec<Finding>) {
    let mut line = 1;
    let mut column = 1;
    for character in text.chars() {
        if let Some(name) = forbidden_unicode_name(character) {
            findings.push(Finding {
                code: "E_UNICODE",
                path: path.to_path_buf(),
                line: Some(line),
                column: Some(column),
                message: format!(
                    "forbidden invisible/bidirectional Unicode U+{:04X} ({name})",
                    character as u32
                ),
            });
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
}

fn forbidden_unicode_name(character: char) -> Option<&'static str> {
    match character as u32 {
        0x00AD => Some("SOFT HYPHEN"),
        0x034F => Some("COMBINING GRAPHEME JOINER"),
        0x061C => Some("ARABIC LETTER MARK"),
        0x180E => Some("MONGOLIAN VOWEL SEPARATOR"),
        0x200B => Some("ZERO WIDTH SPACE"),
        0x200C => Some("ZERO WIDTH NON-JOINER"),
        0x200D => Some("ZERO WIDTH JOINER"),
        0x200E..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 => Some("BIDIRECTIONAL CONTROL"),
        0x2060..=0x2065 | 0x206A..=0x206F => Some("INVISIBLE FORMAT CONTROL"),
        0xFE00..=0xFE0F | 0xE0100..=0xE01EF => Some("VARIATION SELECTOR"),
        0xFEFF => Some("ZERO WIDTH NO-BREAK SPACE"),
        0xFFF9..=0xFFFB => Some("INTERLINEAR ANNOTATION CONTROL"),
        _ => None,
    }
}

fn api_requirement_supported(requirement: &str, current: &str) -> bool {
    let requirement = requirement.trim();
    let (caret, version) = match requirement.strip_prefix('^') {
        Some(version) => (true, version),
        None => (false, requirement),
    };
    let Some(required) = parse_version(version) else {
        return false;
    };
    let Some(current) = parse_version(current) else {
        return false;
    };
    if caret {
        required.0 == current.0 && current >= required
    } else {
        current == required
    }
}

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    parts.next().is_none().then_some((major, minor, patch))
}

fn is_ignored_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| IGNORED_DIRECTORIES.contains(&name))
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|left, right| {
        display_path(&left.path)
            .cmp(&display_path(&right.path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
            .then(left.code.cmp(right.code))
            .then(left.message.cmp(&right.message))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scaffold() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("src")).unwrap();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.clean","name":"Clean","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js",
              "permissions":[],"activation":["onShortcut:open"],
              "contributes":{"shortcuts":[{"id":"open","label":"Open"}]}
            }"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("src/main.ts"),
            "grain.log.info('ok');\n",
        )
        .unwrap();
        directory
    }

    #[test]
    fn fresh_unbuilt_scaffold_has_zero_findings() {
        let directory = scaffold();
        let report = doctor(directory.path());
        assert_eq!(report.findings, Vec::new());
        assert_eq!(report.files_checked, 2);
        assert!(report.to_string().starts_with("doctor: 0 findings"));
    }

    #[test]
    fn zero_width_character_reports_file_line_column_and_codepoint() {
        let directory = scaffold();
        fs::write(
            directory.path().join("src/main.ts"),
            "const ok = true;\nconst hidden = '\u{200b}';\n",
        )
        .unwrap();
        let report = doctor(directory.path());
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == "E_UNICODE")
            .unwrap();
        assert_eq!(finding.path, PathBuf::from("src/main.ts"));
        assert_eq!(finding.line, Some(2));
        assert_eq!(finding.column, Some(17));
        assert!(finding.message.contains("U+200B"));
    }

    #[test]
    fn bidi_and_variation_selectors_are_rejected() {
        let directory = scaffold();
        fs::write(
            directory.path().join("src/main.ts"),
            "const a = '\u{202e}'; const b = '\u{fe0f}';",
        )
        .unwrap();
        let report = doctor(directory.path());
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.code == "E_UNICODE")
                .count(),
            2
        );
    }

    #[test]
    fn manifest_capability_activation_and_budget_failures_accumulate() {
        let directory = scaffold();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.bad","name":"Bad","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js",
              "permissions":["not-real","surface:overlay"],
              "activation":["onShortcut:missing","onEvent:NoSuchEvent"],
              "surfaces":{"overlay":{"size":[900,600],"timeout_ms":20000,"ui_source":"<p>x</p>"}},
              "contributes":{"shortcuts":[{"id":"open","label":"Open"}]}
            }"#,
        )
        .unwrap();
        let report = doctor(directory.path());
        for code in ["E_MANIFEST", "E_ACTIVATION", "E_BUDGET"] {
            assert!(
                report.findings.iter().any(|finding| finding.code == code),
                "missing {code}: {report}"
            );
        }
    }

    #[test]
    fn event_and_transform_activations_require_their_permissions() {
        let directory = scaffold();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.unguarded","name":"Unguarded","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js",
              "permissions":[],
              "activation":["onEvent:TranscriptionComplete","onTransform"]
            }"#,
        )
        .unwrap();
        let report = doctor(directory.path());
        let messages = report
            .findings
            .iter()
            .filter(|finding| finding.code == "E_CAPABILITY")
            .map(|finding| finding.message.as_str())
            .collect::<Vec<_>>();
        assert!(
            messages
                .iter()
                .any(|message| message.contains("events:transcripts")),
            "{report}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("transform:transcript")),
            "{report}"
        );
    }

    #[test]
    fn native_developer_project_has_no_javascript_entry_requirement() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.native","name":"Native","version":"0.1.0",
              "grainApi":"^1.0","tier":"native","activation":["onStartup"],
              "companion":{"windows":"bin/native.exe","macos":"bin/native","linux":"bin/native"}
            }"#,
        )
        .unwrap();

        assert!(doctor(directory.path()).is_clean());
    }

    #[test]
    fn session_mode_is_a_valid_runtime_activation_path() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.note","name":"Note","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js",
              "permissions":["session:start"],"activation":[],
              "contributes":{"sessionMode":{"id":"note","label":"Note"}}
            }"#,
        )
        .unwrap();

        assert!(doctor(directory.path()).is_clean());
    }

    #[test]
    fn ignored_dependency_and_build_directories_are_not_scanned() {
        let directory = scaffold();
        for ignored in ["node_modules", "dist", ".git", "target"] {
            fs::create_dir(directory.path().join(ignored)).unwrap();
            fs::write(directory.path().join(ignored).join("hidden.js"), "\u{200b}").unwrap();
        }
        assert!(doctor(directory.path()).is_clean());
    }
}
