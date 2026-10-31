//! Shared extension-project checks used by `grain-ext doctor` and registry CI.
//!
//! The checker is filesystem-only and returns deterministic findings instead of
//! printing or exiting. CI and the local CLI therefore consume identical code
//! and can choose their own presentation without duplicating policy.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use grain_sdk::compatibility::api_requirement_supported;
use grain_sdk::{ExtensionProjectManifest, GrainPack, PackPayloads, Tier, GRAIN_API_VERSION};

pub mod listing;

/// Strict bounded TOML shared by author CLI and maintainer checks.
pub fn parse_submission(raw: &str) -> Result<grain_sdk::submission::SourceSubmission, String> {
    if raw.len() > grain_sdk::submission::SUBMISSION_MAX_BYTES {
        return Err("submission.toml exceeds its byte limit.".into());
    }
    let submission: grain_sdk::submission::SourceSubmission =
        toml::from_str(raw).map_err(|e| format!("Invalid submission TOML: {e}"))?;
    submission.validate()?;
    Ok(submission)
}

pub fn serialize_submission(
    submission: &grain_sdk::submission::SourceSubmission,
) -> Result<String, String> {
    submission.validate()?;
    let raw = toml::to_string_pretty(submission).map_err(|e| e.to_string())?;
    parse_submission(&raw)?;
    Ok(raw)
}

pub fn read_submission(
    root: &Path,
    expected_id: &str,
) -> Result<grain_sdk::submission::SourceSubmission, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = safe_project_file(&root, "submission.toml", "submission")?;
    if fs::symlink_metadata(root.join("submission.toml"))
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("submission.toml must not be a symbolic link.".into());
    }
    let raw = read_bounded_utf8(
        &path,
        grain_sdk::submission::SUBMISSION_MAX_BYTES as u64,
        "submission.toml",
    )?;
    let submission = parse_submission(&raw)?;
    if submission.id != expected_id {
        return Err("Submission id must match its registry folder.".into());
    }
    listing::verify_submission_listing(&root, &submission)?;
    Ok(submission)
}

pub struct ProjectIdentity {
    pub kind: grain_sdk::distribution::ArtifactKind,
    pub id: String,
    pub version: String,
    pub grain_api: String,
    pub summary: String,
}

/// Use bounded/contained metadata reads after doctor, not a second unbounded
/// manifest read from author-controlled source paths.
pub fn project_identity(root: &Path) -> Result<ProjectIdentity, String> {
    if is_mcp_project(root)? {
        let descriptor = read_mcp_descriptor(root)?;
        return Ok(ProjectIdentity {
            kind: grain_sdk::distribution::ArtifactKind::McpDescriptor,
            id: descriptor.id,
            version: descriptor.version,
            grain_api: descriptor.grain_api,
            summary: descriptor.description,
        });
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = safe_project_file(&root, "manifest.json", "manifest")?;
    let raw = read_bounded_utf8(&path, MAX_MANIFEST_BYTES, "manifest.json")?;
    let project: ExtensionProjectManifest =
        serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    check_project_identity(&project)?;
    let manifest = project.manifest;
    Ok(ProjectIdentity {
        kind: grain_sdk::distribution::ArtifactKind::Native,
        id: manifest.id,
        version: manifest.version,
        grain_api: manifest.grain_api,
        summary: if manifest.description.is_empty() {
            manifest.name
        } else {
            manifest.description
        },
    })
}

fn check_project_identity(project: &ExtensionProjectManifest) -> Result<(), String> {
    let mut manifest = project.manifest.clone();
    manifest.entry_source = "// source submitted for a separate reviewed build".into();
    GrainPack {
        manifest,
        payloads: PackPayloads::default(),
    }
    .validate()
}

pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_PROJECT_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = MAX_PROJECT_FILE_BYTES;

/// An MCP project contains metadata, never a JavaScript entry or host identity.
/// Refuse ambiguous roots rather than silently choosing one extension kind.
pub fn is_mcp_project(root: &Path) -> Result<bool, String> {
    let mcp = root
        .join("mcp.json")
        .try_exists()
        .map_err(|e| e.to_string())?;
    let native = root
        .join("manifest.json")
        .try_exists()
        .map_err(|e| e.to_string())?;
    if mcp && native {
        return Err("project must contain either manifest.json or mcp.json, not both".into());
    }
    Ok(mcp)
}

/// The exact host parser, bounded filesystem read and canonical descriptor are
/// shared by doctor, CLI packaging and registry tooling. This performs no DNS,
/// network, registration or execution; destination admission remains host-owned.
pub fn read_mcp_descriptor(root: &Path) -> Result<grain_sdk::mcp::McpDescriptor, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("open project root: {e}"))?;
    if !is_mcp_project(&root)? {
        return Err("MCP project requires mcp.json".into());
    }
    let path = safe_project_file(&root, "mcp.json", "MCP descriptor")?;
    let bytes = read_bounded(
        &path,
        grain_sdk::mcp::MCP_DESCRIPTOR_MAX_BYTES as u64,
        "mcp.json",
    )?;
    grain_core::mcp::ValidatedDescriptor::parse(&bytes)
        .map(|value| value.descriptor().clone())
        .map_err(|e| e.to_string())
}

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
        write!(formatter, " error [{}] {}", self.code, self.message)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoctorReport {
    pub findings: Vec<Finding>,
    pub files_checked: usize,
}

impl DoctorReport {
    /// Every finding blocks submission.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

impl fmt::Display for DoctorReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.findings.is_empty() {
            return write!(
                formatter,
                "doctor: 0 findings ({} files checked)",
                self.files_checked
            );
        }
        writeln!(
            formatter,
            "doctor: {} error(s) ({} files checked)",
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
    if root.join("DESCRIPTION.md").exists() || root.join("media").exists() {
        if let Err(error) = listing::read_listing(&root) {
            report
                .findings
                .push(Finding::project("E_LISTING", "DESCRIPTION.md", error));
        }
    }
    match is_mcp_project(&root) {
        Ok(true) => {
            if let Err(error) = read_mcp_descriptor(&root) {
                report
                    .findings
                    .push(Finding::project("E_MCP_DESCRIPTOR", "mcp.json", error));
            }
        }
        Ok(false) => check_manifest(&root, &mut report),
        Err(error) => report
            .findings
            .push(Finding::project("E_PROJECT_KIND", ".", error)),
    }
    sort_findings(&mut report.findings);
    report
}

/// Build the single-file artifact registry CI and local imports consume.
/// Compilation is intentionally the caller's job; this function is the shared,
/// deterministic packaging boundary used after a scripted entry has been built.
pub fn build_pack(root: &Path) -> Result<GrainPack, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("open project root: {error}"))?;
    let report = doctor(&root);
    if !report.is_clean() {
        return Err(format!("doctor found problems:\n{report}"));
    }
    let manifest_path = safe_project_file(&root, "manifest.json", "manifest")?;
    let raw = read_bounded_utf8(&manifest_path, MAX_MANIFEST_BYTES, "manifest.json")?;
    let project: ExtensionProjectManifest =
        serde_json::from_str(&raw).map_err(|error| format!("parse manifest.json: {error}"))?;
    if project.manifest.tier == Tier::Native {
        return Err("native developer projects cannot be packaged in V1".into());
    }

    let mut manifest = project.manifest;
    if manifest.tier == Tier::Scripted {
        let entry = safe_project_file(&root, &project.entry, "entry")?;
        manifest.entry_source = read_bounded_utf8(&entry, MAX_ENTRY_BYTES, &project.entry)
            .map_err(|error| format!("{error}; run the project build first"))?;
    }

    let icon = safe_project_file(&root, &manifest.icon, "icon")?;
    let icon_bytes = read_bounded(&icon, grain_sdk::ICON_MAX_BYTES, &manifest.icon)?;
    let pack = GrainPack {
        manifest,
        payloads: PackPayloads {
            icon_png: Some(base64::engine::general_purpose::STANDARD.encode(icon_bytes)),
            ..PackPayloads::default()
        },
    };
    pack.validate()?;
    Ok(pack)
}

fn read_bounded(path: &Path, max: u64, label: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let file = fs::File::open(path).map_err(|error| format!("read {label}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    if bytes.len() as u64 > max {
        return Err(format!("{label} exceeds its {max}-byte limit"));
    }
    Ok(bytes)
}

fn read_bounded_utf8(path: &Path, max: u64, label: &str) -> Result<String, String> {
    String::from_utf8(read_bounded(path, max, label)?)
        .map_err(|_| format!("{label} is not valid UTF-8"))
}

fn safe_project_file(root: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("{label} must be a project-relative file"));
    }
    let joined = root.join(path);
    if let Ok(canonical) = joined.canonicalize() {
        if !canonical.starts_with(root) {
            return Err(format!("{label} resolves outside the project"));
        }
        Ok(canonical)
    } else {
        Ok(joined)
    }
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
    check_actions(&project, report);
    check_icon(root, &project, report);
}

/// [GRAIN] The icon is mandatory to submit and standardised to one shape
/// (`docs/Extensions V1/PLAN.md` §13.3): a square PNG at exactly
/// [`grain_sdk::ICON_MASTER_DIM`]², under [`grain_sdk::ICON_MAX_BYTES`]. Grain
/// downscales that master everywhere it shows an icon, so the author supplies one
/// file and consistency is Grain's.
///
/// Errors (blocks submission), never warns: an extension with no recognisable
/// icon is exactly what the store must not list. The cheap header check rejects
/// the wrong format or dimensions before a full decode proves the PNG is intact.
fn check_icon(root: &Path, project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    use grain_sdk::{ICON_MASTER_DIM, ICON_MAX_BYTES};

    let declared = project.manifest.icon.trim();
    if declared.is_empty() {
        report.findings.push(Finding::project(
            "E_ICON",
            "manifest.json",
            format!(
                "an icon is required to submit: declare \"icon\" and ship a {ICON_MASTER_DIM}×\
                 {ICON_MASTER_DIM} square PNG"
            ),
        ));
        return;
    }

    // Project-relative and path-safe, exactly like `entry`.
    let path = Path::new(declared);
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        report.findings.push(Finding::project(
            "E_ICON",
            "manifest.json",
            "icon must be a project-relative file",
        ));
        return;
    }

    let icon_path = root.join(path);
    let bytes = match fs::read(&icon_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            report.findings.push(Finding::project(
                "E_ICON",
                PathBuf::from(declared),
                "icon file not found",
            ));
            return;
        }
    };
    if bytes.len() as u64 > ICON_MAX_BYTES {
        report.findings.push(Finding::project(
            "E_SIZE",
            PathBuf::from(declared),
            format!("icon is larger than {} KB", ICON_MAX_BYTES / 1024),
        ));
        return;
    }
    match grain_sdk::png_dimensions(&bytes) {
        Some((width, height)) if width == ICON_MASTER_DIM && height == ICON_MASTER_DIM => {
            if image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).is_err() {
                report.findings.push(Finding::project(
                    "E_ICON",
                    PathBuf::from(declared),
                    "icon PNG is corrupt or truncated",
                ));
            }
        }
        Some((width, height)) => report.findings.push(Finding::project(
            "E_ICON",
            PathBuf::from(declared),
            format!(
                "icon is {width}×{height}; it must be exactly {ICON_MASTER_DIM}×{ICON_MASTER_DIM}"
            ),
        )),
        None => report.findings.push(Finding::project(
            "E_ICON",
            PathBuf::from(declared),
            "icon is not a PNG",
        )),
    }
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
            grain_sdk::compatibility::UNSUPPORTED_EXTENSION_API,
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
    // Developer and install validation both enforce the tool-only boundary.
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

fn check_actions(project: &ExtensionProjectManifest, report: &mut DoctorReport) {
    if project.manifest.contributes.actions.is_empty() {
        report.findings.push(Finding::project(
            "E_ACTIONS",
            "manifest.json",
            "native tool extensions require at least one declared action",
        ));
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
            // `grain-ext pack` writes its generated deliverable in the project
            // root by default. It is not submitted source and can legitimately
            // exceed the per-source-file ceiling once entry + base64 icon are
            // combined; repeated doctor/pack runs must remain idempotent.
            if path.extension().and_then(|value| value.to_str()) == Some("grainpack") {
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

    #[test]
    fn typed_submission_serialization_is_exact_strict_and_bounded() {
        use grain_sdk::submission::*;
        let submission = SourceSubmission {
            schema: 1,
            artifact_kind: grain_sdk::distribution::ArtifactKind::Native,
            id: "com.example.tools".into(),
            version: "1.0.0".into(),
            grain_api: "^1.0".into(),
            source_repo: "https://github.com/example/tools".into(),
            tag: "v1.0.0".into(),
            commit: "1".repeat(40),
            summary: "Say \"hello\" from C:\\tools".into(),
            categories: vec!["tools".into()],
            license: "MIT".into(),
            contact: "a@example.com".into(),
            description_sha256: "a".repeat(64),
            description_size: 10,
            media: Vec::new(),
        };
        let raw = serialize_submission(&submission).unwrap();
        assert_eq!(parse_submission(&raw).unwrap().summary, submission.summary);
        assert!(parse_submission(&(raw.clone() + "\ntrust = 'core'\n")).is_err());
        assert!(parse_submission(&(raw.clone() + "\nid = 'com.other.tools'\n")).is_err());
        assert!(parse_submission(&" ".repeat(SUBMISSION_MAX_BYTES + 1)).is_err());
        assert!(parse_submission(
            "id = 'com.example.legacy'\nsource_repo = 'https://github.com/example/legacy'\n"
        )
        .is_err());
    }

    #[test]
    fn mcp_doctor_uses_host_admission_and_bounds_without_native_requirements() {
        let directory = tempfile::tempdir().unwrap();
        let descriptor = serde_json::json!({"schema":1,"id":"com.example.mcp","name":"Tools","description":"Remote tools","version":"1.0.0","grainApi":"^1.0","transport":{"type":"streamable-http","url":"https://tools.example/mcp"},"authentication":{"type":"none"}});
        let path = directory.path().join("mcp.json");
        fs::write(&path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        assert!(doctor(directory.path()).is_clean());
        assert_eq!(doctor(directory.path()).files_checked, 1);
        for (key, value) in [
            ("schema", serde_json::json!(2)),
            ("grainApi", serde_json::json!("^2.0")),
            ("id", serde_json::json!("../escape")),
            (
                "transport",
                serde_json::json!({"type":"stdio","command":"app"}),
            ),
            (
                "authentication",
                serde_json::json!({"type":"none","token":"secret"}),
            ),
            ("enabled", serde_json::json!(true)),
        ] {
            let mut bad = descriptor.clone();
            bad[key] = value;
            fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
            let report = doctor(directory.path());
            assert!(!report.is_clean(), "unexpected admission of {key}");
            assert!(report.findings.iter().any(|f| f.code == "E_MCP_DESCRIPTOR"));
        }
        fs::write(
            &path,
            vec![b' '; grain_sdk::mcp::MCP_DESCRIPTOR_MAX_BYTES + 1],
        )
        .unwrap();
        assert!(read_mcp_descriptor(directory.path())
            .unwrap_err()
            .contains("8192"));
        fs::write(&path, b"{invalid").unwrap();
        assert!(!doctor(directory.path()).is_clean());
    }

    fn fake_png(dim: u32) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(dim, dim))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    fn write_icon(dir: &Path) {
        fs::write(dir.join("icon.png"), fake_png(grain_sdk::ICON_MASTER_DIM)).unwrap();
    }

    fn scaffold() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("src")).unwrap();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.clean","name":"Clean","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js","icon":"icon.png",
              "permissions":[],
              "contributes":{"actions":[{"id":"read","title":"Read","risk":"safe","utterances":["read data"]}]}
            }"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("src/main.ts"),
            "grain.log.info('ok');\n",
        )
        .unwrap();
        write_icon(directory.path());
        directory
    }

    #[test]
    fn fresh_unbuilt_scaffold_has_zero_findings() {
        let directory = scaffold();
        let report = doctor(directory.path());
        assert_eq!(report.findings, Vec::new());
        assert_eq!(report.files_checked, 3);
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
    fn manifest_and_missing_action_failures_accumulate() {
        let directory = scaffold();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.bad","name":"Bad","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js",
              "permissions":["not-real"]
            }"#,
        )
        .unwrap();
        let report = doctor(directory.path());
        for code in ["E_MANIFEST", "E_ACTIONS"] {
            assert!(
                report.findings.iter().any(|finding| finding.code == code),
                "missing {code}: {report}"
            );
        }
    }

    #[test]
    fn native_companions_are_rejected_in_developer_projects() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.native","name":"Native","version":"0.1.0",
              "grainApi":"^1.0","tier":"native","permissions":["resident"],
              "activation":["onStartup"],"icon":"icon.png",
              "companion":{"windows":"bin/native.exe","macos":"bin/native","linux":"bin/native"}
            }"#,
        )
        .unwrap();
        write_icon(directory.path());

        assert!(doctor(directory.path())
            .findings
            .iter()
            .any(|finding| finding.code == "E_MANIFEST" && finding.message.contains("Retired")));
    }

    #[test]
    fn retired_session_modes_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("manifest.json"),
            r#"{
              "id":"com.example.note","name":"Note","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js","icon":"icon.png",
              "permissions":["session:start"],"activation":[],
              "contributes":{"sessionMode":{"id":"note","label":"Note"}}
            }"#,
        )
        .unwrap();
        write_icon(directory.path());

        assert!(doctor(directory.path())
            .findings
            .iter()
            .any(|finding| finding.code == "E_MANIFEST" && finding.message.contains("Retired")));
    }

    #[test]
    fn declared_actions_are_sufficient_for_tools() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("manifest.json"), TOOLS).unwrap();
        write_icon(directory.path());

        let report = doctor(directory.path());
        assert!(report.findings.is_empty(), "{report}");
    }

    #[test]
    fn tools_without_actions_cannot_be_packaged() {
        let directory = scaffold();
        let path = directory.path().join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["contributes"]["actions"] = serde_json::json!([]);
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let report = doctor(directory.path());
        assert!(!report.is_clean());
        assert!(codes(&report).contains(&"E_ACTIONS"), "{report}");
        assert!(build_pack(directory.path())
            .unwrap_err()
            .contains("E_ACTIONS"));
    }

    /// A well-formed tool extension, as the checks below vary it. Pair with
    /// [`write_icon`] — every submittable extension needs one (§13.3).
    const TOOLS: &str = r#"{
      "id":"com.example.open","name":"Open","version":"1.1.0",
      "grainApi":"^1.0","tier":"scripted","entry":"dist/main.js","icon":"icon.png",
      "permissions":["storage"],
      "contributes":{"actions":[{
        "id":"open","title":"Open a site you set up",
        "risk":"safe","utterances":["open {target}","launch {target}"],
        "params":[{"name":"target","kind":"entity","resolve":true}]}]}
    }"#;

    fn variant(replace: &[(&str, &str)]) -> tempfile::TempDir {
        let mut manifest = TOOLS.to_string();
        for (from, to) in replace {
            assert!(manifest.contains(from), "fixture no longer contains {from}");
            manifest = manifest.replace(from, to);
        }
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("manifest.json"), manifest).unwrap();
        write_icon(directory.path());
        directory
    }

    fn codes(report: &DoctorReport) -> Vec<&'static str> {
        report.findings.iter().map(|f| f.code).collect()
    }

    #[test]
    fn an_extension_cannot_be_submitted_without_an_icon() {
        // Drop the icon field from the fixture. The file is still on disk, but
        // nothing declares it — the store must not list an extension with no
        // recognisable icon (§13.3).
        let directory = variant(&[(r#""icon":"icon.png","#, "")]);
        let report = doctor(directory.path());
        assert!(codes(&report).contains(&"E_ICON"), "{report}");
        assert!(!report.is_clean(), "a missing icon blocks submission");
    }

    #[test]
    fn a_declared_icon_that_is_not_on_disk_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("manifest.json"), TOOLS).unwrap();
        // Deliberately do NOT write_icon.
        let report = doctor(directory.path());
        let icon = report
            .findings
            .iter()
            .find(|f| f.code == "E_ICON")
            .expect("missing file is E_ICON");
        assert!(icon.message.contains("not found"), "{}", icon.message);
    }

    #[test]
    fn an_icon_of_the_wrong_size_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("manifest.json"), TOOLS).unwrap();
        fs::write(directory.path().join("icon.png"), fake_png(256)).unwrap();
        let report = doctor(directory.path());
        let icon = report
            .findings
            .iter()
            .find(|f| f.code == "E_ICON")
            .expect("256x256 is not the 512 master");
        assert!(icon.message.contains("512"), "{}", icon.message);
    }

    #[test]
    fn a_non_png_icon_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("manifest.json"), TOOLS).unwrap();
        fs::write(
            directory.path().join("icon.png"),
            b"GIF89a not really a png",
        )
        .unwrap();
        let report = doctor(directory.path());
        let icon = report
            .findings
            .iter()
            .find(|f| f.code == "E_ICON")
            .expect("a non-PNG is E_ICON");
        assert!(icon.message.contains("not a PNG"), "{}", icon.message);
    }

    #[test]
    fn a_valid_512_master_passes() {
        // The happy path: the fixture ships a 512×512 PNG and the icon check is
        // silent. (variant writes the icon; TOOLS declares it.)
        let report = doctor(variant(&[]).path());
        assert!(!codes(&report).contains(&"E_ICON"), "{report}");
    }

    #[test]
    fn a_truncated_png_with_a_valid_ihdr_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("manifest.json"), TOOLS).unwrap();
        let mut bytes = vec![0u8; 24];
        bytes[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&grain_sdk::ICON_MASTER_DIM.to_be_bytes());
        bytes[20..24].copy_from_slice(&grain_sdk::ICON_MASTER_DIM.to_be_bytes());
        fs::write(directory.path().join("icon.png"), bytes).unwrap();

        let report = doctor(directory.path());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "E_ICON" && finding.message.contains("corrupt")));
    }

    #[test]
    fn build_pack_embeds_the_checked_icon_and_built_entry() {
        let directory = scaffold();
        fs::create_dir(directory.path().join("dist")).unwrap();
        fs::write(
            directory.path().join("dist/main.js"),
            "globalThis.grainExtension = {};",
        )
        .unwrap();

        let pack = build_pack(directory.path()).unwrap();
        assert_eq!(
            pack.manifest.entry_source,
            "globalThis.grainExtension = {};"
        );
        let icon = pack.embedded_icon_png().unwrap().unwrap();
        assert_eq!(
            grain_sdk::png_dimensions(&icon),
            Some((grain_sdk::ICON_MASTER_DIM, grain_sdk::ICON_MASTER_DIM))
        );
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

    #[test]
    fn generated_grainpack_artifacts_are_not_rescanned_as_source() {
        let directory = scaffold();
        fs::write(
            directory.path().join("com.example.clean-0.1.0.grainpack"),
            vec![b'x'; MAX_PROJECT_FILE_BYTES as usize + 1],
        )
        .unwrap();

        assert!(doctor(directory.path()).is_clean());
    }
}
