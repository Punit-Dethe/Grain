//! [GRAIN] Load-unpacked project reader (Phase 3.5).
//!
//! This module is deliberately filesystem-only: the native folder picker and
//! registry mutation stay in Tauri commands, while path containment, API
//! compatibility, source size, and manifest validation remain pure and tested.

use std::path::{Component, Path, PathBuf};

use grain_sdk::{ExtensionProjectManifest, GrainPack, PackPayloads, Tier, GRAIN_API_VERSION};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug)]
pub struct LoadedDevProject {
    pub root: PathBuf,
    /// Canonical built JavaScript entry. The worker host keeps only this path
    /// and resolves its source-map reference lazily if the worker throws.
    pub entry_path: PathBuf,
    pub pack: GrainPack,
}

pub fn load_project(root: &Path) -> Result<LoadedDevProject, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("open project folder: {error}"))?;
    if !root.is_dir() {
        return Err("the selected project is not a folder".into());
    }

    let manifest_path = root.join("manifest.json");
    let manifest_meta = std::fs::metadata(&manifest_path)
        .map_err(|error| format!("read manifest.json: {error}"))?;
    if manifest_meta.len() > MAX_MANIFEST_BYTES {
        return Err("manifest.json is larger than 1 MB".into());
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("read manifest.json: {error}"))?;
    let mut project: ExtensionProjectManifest =
        serde_json::from_str(&raw).map_err(|error| format!("parse manifest.json: {error}"))?;

    if project.manifest.tier != Tier::Scripted {
        return Err("load unpacked currently supports scripted extensions".into());
    }
    if !project.manifest.entry_source.is_empty() {
        return Err("manifest.json must use 'entry', not embedded 'entry_source'".into());
    }
    if !api_requirement_supported(&project.manifest.grain_api, GRAIN_API_VERSION) {
        return Err(format!(
            "extension requires Grain API '{}', but this build provides '{}'",
            project.manifest.grain_api, GRAIN_API_VERSION
        ));
    }

    let relative_entry = Path::new(&project.entry);
    if relative_entry.as_os_str().is_empty()
        || relative_entry.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("manifest entry must be a project-relative file".into());
    }
    let entry_path = root
        .join(relative_entry)
        .canonicalize()
        .map_err(|error| format!("open entry '{}': {error}", project.entry))?;
    if !entry_path.starts_with(&root) || !entry_path.is_file() {
        return Err("manifest entry must stay inside the project".into());
    }
    let entry_meta = std::fs::metadata(&entry_path)
        .map_err(|error| format!("inspect entry '{}': {error}", project.entry))?;
    if entry_meta.len() > MAX_ENTRY_BYTES {
        return Err("extension entry is larger than 5 MB".into());
    }
    project.manifest.entry_source = std::fs::read_to_string(&entry_path)
        .map_err(|error| format!("read entry '{}': {error}", project.entry))?;

    let pack = GrainPack {
        manifest: project.manifest,
        payloads: PackPayloads::default(),
    };
    pack.validate()
        .map_err(|error| format!("invalid extension: {error}"))?;
    Ok(LoadedDevProject {
        root,
        entry_path,
        pack,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("manifest.json"),
            r#"{
              "id":"com.example.test","name":"Test","version":"0.1.0",
              "grainApi":"^1.0","tier":"scripted","entry":"src/main.ts",
              "permissions":[],"activation":["onShortcut:open"],
              "contributes":{"shortcuts":[{"id":"open","label":"Open"}]}
            }"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("src/main.ts"), "grain.log.info('ready');").unwrap();
        dir
    }

    #[test]
    fn scaffold_shape_loads_and_injects_entry_source() {
        let dir = project();
        let loaded = load_project(dir.path()).unwrap();
        assert_eq!(loaded.pack.manifest.id, "com.example.test");
        assert_eq!(
            loaded.pack.manifest.entry_source,
            "grain.log.info('ready');"
        );
        assert_eq!(loaded.root, dir.path().canonicalize().unwrap());
        assert_eq!(
            loaded.entry_path,
            dir.path().join("src/main.ts").canonicalize().unwrap()
        );
    }

    #[test]
    fn incompatible_or_malformed_api_requirements_are_rejected() {
        assert!(api_requirement_supported("^1.0", "1.0"));
        assert!(api_requirement_supported("^1.0", "1.4"));
        assert!(!api_requirement_supported("^2.0", "1.4"));
        assert!(!api_requirement_supported("1.0", "1.1"));
        assert!(!api_requirement_supported("latest", "1.0"));
    }

    #[test]
    fn entry_cannot_escape_the_project() {
        let dir = project();
        let raw = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        std::fs::write(
            dir.path().join("manifest.json"),
            raw.replace("src/main.ts", "../outside.js"),
        )
        .unwrap();
        assert!(load_project(dir.path())
            .unwrap_err()
            .contains("project-relative"));
    }
}
