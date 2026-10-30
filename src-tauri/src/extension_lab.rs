//! Retired whole-request Recommendation Lab. Keep cleanup paths for existing data.
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const LAB_PREFIX: &str = "com.grain.lab.";
pub const CORE_COUNT: usize = 6;
pub const STRESS_COUNT: usize = 24;
const LAB_DIRECTORY: &str = ".recommendation-lab-v1";
const LAB_MARKER: &str = ".grain-recommendation-lab";
const LAB_MARKER_VALUE: &str = "grain-recommendation-lab-v1\n";
#[derive(Debug)]
pub struct LabProject {
    pub id: String,
    pub root: PathBuf,
}
pub fn ids(limit: usize) -> Vec<String> {
    [
        "stream-music",
        "music-library",
        "issue-tracker",
        "code-host",
        "translator",
        "calendar",
        "team-chat",
        "email",
        "meeting-notes",
        "document-notes",
        "task-list",
        "knowledge-base",
        "weather",
        "maps",
        "contacts",
        "calculator",
        "browser-search",
        "file-organizer",
        "timer",
        "smart-home",
        "video-meeting",
        "clipboard-tools",
        "expense-tracker",
        "travel-planner",
    ]
    .iter()
    .take(limit.min(STRESS_COUNT))
    .map(|slug| format!("{LAB_PREFIX}{slug}"))
    .collect()
}
pub fn materialize(_app: &AppHandle) -> Result<Vec<LabProject>, String> {
    Err("The whole-request Recommendation Lab is retired. Use tool-only test extensions.".into())
}

pub fn root(app: &AppHandle) -> Result<PathBuf, String> {
    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    Ok(ctx.data_dir.join("extensions").join(LAB_DIRECTORY))
}

pub fn owns_project(app: &AppHandle, path: &Path) -> bool {
    let Ok(root) = root(app).and_then(|path| path.canonicalize().map_err(|e| e.to_string())) else {
        return false;
    };
    path.canonicalize().is_ok_and(|path| path.starts_with(root))
}

pub fn remove_materialized(app: &AppHandle) -> Result<(), String> {
    let root = root(app)?;
    if !root.exists() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("refusing to remove a non-directory Recommendation Lab path".into());
    }
    let marker = std::fs::read_to_string(root.join(LAB_MARKER))
        .map_err(|_| "refusing to remove an unmarked Recommendation Lab directory".to_string())?;
    if marker != LAB_MARKER_VALUE {
        return Err(
            "refusing to remove a Recommendation Lab directory with an unknown marker".into(),
        );
    }
    let parent = root
        .parent()
        .ok_or("lab root has no parent")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let canonical = root.canonicalize().map_err(|error| error.to_string())?;
    if canonical.parent() != Some(parent.as_path())
        || canonical.file_name().and_then(|name| name.to_str()) != Some(LAB_DIRECTORY)
    {
        return Err(
            "refusing to remove a Recommendation Lab directory outside the fixed root".into(),
        );
    }
    std::fs::remove_dir_all(canonical).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_lab_ids_remain_available_for_cleanup_only() {
        assert_eq!(ids(STRESS_COUNT).len(), STRESS_COUNT);
        assert_eq!(ids(CORE_COUNT).len(), CORE_COUNT);
        assert_eq!(ids(usize::MAX).len(), STRESS_COUNT);
    }
}
