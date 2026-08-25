//! [GRAIN] Debug-only searchable-extension corpus for real-application testing.
//!
//! The lab does not add a second recommendation or rendering path. It writes
//! ordinary load-unpacked projects, then the existing developer loader,
//! registry, worker host, recommendation index, icon derivation, chooser, and
//! standard surface consume them exactly as they consume author projects.

use std::path::{Path, PathBuf};

use grain_sdk::{ExtensionProjectManifest, GrainPack, PackPayloads};
use serde_json::json;
use tauri::{AppHandle, Manager};

pub const LAB_PREFIX: &str = "com.grain.lab.";
pub const CORE_COUNT: usize = 6;
pub const STRESS_COUNT: usize = 24;

const LAB_DIRECTORY: &str = ".recommendation-lab-v1";
const LAB_MARKER: &str = ".grain-recommendation-lab";
const LAB_MARKER_VALUE: &str = "grain-recommendation-lab-v1\n";
const LAB_RUNTIME: &str = include_str!("extension_lab_runtime.js");

#[derive(Clone, Copy)]
struct Profile {
    slug: &'static str,
    name: &'static str,
    purpose: &'static str,
    examples: &'static [&'static str],
    aliases: &'static [&'static str],
    entities: &'static [&'static str],
    command_count: u8,
    auto_send: bool,
}

#[derive(Debug)]
pub struct LabProject {
    pub id: String,
    pub root: PathBuf,
}

fn profiles() -> &'static [Profile] {
    &[
        Profile {
            slug: "stream-music",
            name: "Stream Music",
            purpose: "Play and control music from a streaming service",
            examples: &[
                "play some jazz",
                "skip this song",
                "pause the music",
                "play my focus playlist",
                "turn the volume down",
            ],
            aliases: &["stream music", "music streaming"],
            entities: &["artist", "album", "playlist", "track"],
            command_count: 7,
            auto_send: false,
        },
        Profile {
            slug: "music-library",
            name: "Music Library",
            purpose: "Play and organize music stored in a personal library",
            examples: &[
                "play my downloaded jazz",
                "shuffle my albums",
                "add this track to my library",
                "show recently added music",
                "play the local version",
            ],
            aliases: &["local music", "my music library"],
            entities: &["artist", "album", "playlist", "track"],
            command_count: 7,
            auto_send: false,
        },
        Profile {
            slug: "issue-tracker",
            name: "Issue Tracker",
            purpose: "Create and update software issues and project work",
            examples: &[
                "create a bug for the login crash",
                "move this issue to in progress",
                "assign the ticket to Maya",
                "set the issue priority to urgent",
                "show my open project tasks",
            ],
            aliases: &["tickets", "project issues"],
            entities: &["issue", "project", "assignee", "priority"],
            command_count: 8,
            auto_send: false,
        },
        Profile {
            slug: "code-host",
            name: "Code Host",
            purpose: "Manage repositories, pull requests, reviews, and code issues",
            examples: &[
                "open a pull request for this branch",
                "request a code review",
                "create a repository issue",
                "merge the approved pull request",
                "show failing checks",
            ],
            aliases: &["git host", "repositories"],
            entities: &["repository", "branch", "pull request", "review"],
            command_count: 8,
            auto_send: false,
        },
        Profile {
            slug: "translator",
            name: "Translator - Multilingual",
            purpose: "Translate text and short messages between languages",
            examples: &[
                "translate this into Spanish",
                "how do I say thank you in Japanese",
                "translate the selected paragraph",
                "make this message French",
                "convert this to English",
            ],
            aliases: &["translate", "translation"],
            entities: &["language", "text"],
            command_count: 6,
            auto_send: true,
        },
        Profile {
            slug: "calendar",
            name: "Calendar Planner",
            purpose: "Create, move, and review calendar events and availability",
            examples: &[
                "schedule lunch with Priya tomorrow",
                "move my design review to Friday",
                "find a free hour this afternoon",
                "cancel the morning standup",
                "show next week's calendar",
            ],
            aliases: &["calendar", "schedule"],
            entities: &["event", "person", "date", "time"],
            command_count: 11,
            auto_send: false,
        },
        Profile {
            slug: "team-chat",
            name: "Team Chat",
            purpose: "Send and organize messages in team channels",
            examples: &[
                "message the design channel",
                "send Alex a direct message",
                "summarize unread team messages",
                "save this thread for later",
            ],
            aliases: &["team messages"],
            entities: &["person", "channel", "message"],
            command_count: 13,
            auto_send: false,
        },
        Profile {
            slug: "email",
            name: "Email Assistant",
            purpose: "Draft, send, find, and organize email",
            examples: &[
                "email Jordan that I will be late",
                "draft a reply to the latest message",
                "find the invoice email",
                "archive newsletters",
            ],
            aliases: &["mail", "inbox"],
            entities: &["person", "subject", "message"],
            command_count: 14,
            auto_send: false,
        },
        Profile {
            slug: "meeting-notes",
            name: "Meeting Notes",
            purpose: "Capture decisions, summaries, and follow-ups from meetings",
            examples: &[
                "save the decisions from this meeting",
                "write meeting minutes",
                "capture these action items",
                "summarize the standup",
            ],
            aliases: &["minutes"],
            entities: &["meeting", "decision", "action item"],
            command_count: 7,
            auto_send: true,
        },
        Profile {
            slug: "document-notes",
            name: "Document Notes",
            purpose: "Create and organize personal notes and written documents",
            examples: &[
                "make a note about this idea",
                "append this to my project notes",
                "create a document from this outline",
                "find my launch notes",
            ],
            aliases: &["notes", "documents"],
            entities: &["note", "folder", "document"],
            command_count: 9,
            auto_send: false,
        },
        Profile {
            slug: "task-list",
            name: "Task List",
            purpose: "Create, schedule, complete, and prioritize personal tasks",
            examples: &[
                "remind me to renew the domain",
                "add a task for tomorrow",
                "mark the grocery task complete",
                "show overdue tasks",
            ],
            aliases: &["tasks", "to do"],
            entities: &["task", "date", "priority"],
            command_count: 10,
            auto_send: false,
        },
        Profile {
            slug: "knowledge-base",
            name: "Knowledge Base Research and Documentation Assistant",
            purpose: "Search and summarize a team knowledge base",
            examples: &[
                "find the deployment runbook",
                "what is our refund policy",
                "summarize the onboarding guide",
                "look up the incident process",
            ],
            aliases: &["knowledge base", "wiki"],
            entities: &["article", "policy", "runbook"],
            command_count: 8,
            auto_send: true,
        },
        Profile {
            slug: "weather",
            name: "Weather",
            purpose: "Report weather conditions and forecasts",
            examples: &[
                "will it rain tomorrow",
                "what is the temperature outside",
                "show the weekend forecast",
                "is it windy in Mumbai",
            ],
            aliases: &["forecast"],
            entities: &["location", "date"],
            command_count: 5,
            auto_send: true,
        },
        Profile {
            slug: "maps",
            name: "Maps and Directions",
            purpose: "Find places, routes, travel times, and nearby businesses",
            examples: &[
                "navigate to the airport",
                "find coffee near me",
                "how long to drive to Pune",
                "show directions home",
            ],
            aliases: &["maps", "directions"],
            entities: &["place", "route", "business"],
            command_count: 9,
            auto_send: false,
        },
        Profile {
            slug: "contacts",
            name: "Contacts",
            purpose: "Find, create, and update people in an address book",
            examples: &[
                "find Maya's phone number",
                "add Sam to my contacts",
                "update Lee's email address",
                "show the contact for Northwind",
            ],
            aliases: &["address book"],
            entities: &["person", "company", "phone", "email"],
            command_count: 7,
            auto_send: false,
        },
        Profile {
            slug: "calculator",
            name: "Calculator",
            purpose: "Calculate values, convert units, and compare quantities",
            examples: &[
                "what is eighteen percent of 240",
                "convert five miles to kilometres",
                "split this bill four ways",
                "calculate compound interest",
            ],
            aliases: &["calculate", "math"],
            entities: &["number", "unit", "currency"],
            command_count: 8,
            auto_send: true,
        },
        Profile {
            slug: "browser-search",
            name: "Web Search",
            purpose: "Search the web and return a concise set of sources",
            examples: &[
                "search the web for local speech recognition",
                "look up the latest Rust release",
                "find reviews of this microphone",
                "search for that error message",
            ],
            aliases: &["web search", "browser search"],
            entities: &["query", "website"],
            command_count: 5,
            auto_send: true,
        },
        Profile {
            slug: "file-organizer",
            name: "File Organizer",
            purpose: "Find, rename, move, and organize local files",
            examples: &[
                "move these screenshots into the project folder",
                "rename this file",
                "find the latest PDF",
                "organize my downloads",
            ],
            aliases: &["files", "file manager"],
            entities: &["file", "folder", "path"],
            command_count: 15,
            auto_send: false,
        },
        Profile {
            slug: "timer",
            name: "Timer",
            purpose: "Start, stop, and inspect timers and countdowns",
            examples: &[
                "start a timer for twenty minutes",
                "pause the timer",
                "how much time is left",
                "cancel my countdown",
            ],
            aliases: &["countdown"],
            entities: &["duration", "timer"],
            command_count: 4,
            auto_send: false,
        },
        Profile {
            slug: "smart-home",
            name: "Smart Home",
            purpose: "Control approved lights, scenes, and home devices",
            examples: &[
                "turn off the living room lights",
                "set the thermostat to twenty two",
                "activate movie night",
                "lock the front door",
            ],
            aliases: &["home control"],
            entities: &["room", "device", "scene"],
            command_count: 18,
            auto_send: false,
        },
        Profile {
            slug: "video-meeting",
            name: "Video Meeting",
            purpose: "Create, join, and manage video meetings",
            examples: &[
                "start a video call with the team",
                "join my next meeting",
                "copy the meeting link",
                "mute the conference",
            ],
            aliases: &["video call", "conference"],
            entities: &["person", "meeting", "link"],
            command_count: 8,
            auto_send: false,
        },
        Profile {
            slug: "clipboard-tools",
            name: "Clipboard Tools",
            purpose: "Inspect and transform clipboard text locally",
            examples: &[
                "remove formatting from my clipboard",
                "turn the clipboard into a list",
                "copy this as markdown",
                "clean up the copied text",
            ],
            aliases: &["clipboard"],
            entities: &["text", "format"],
            command_count: 6,
            auto_send: true,
        },
        Profile {
            slug: "expense-tracker",
            name: "Expense Tracker",
            purpose: "Record, categorize, and review personal expenses",
            examples: &[
                "record twelve dollars for lunch",
                "categorize this receipt as travel",
                "show spending this week",
                "add a reimbursable expense",
            ],
            aliases: &["expenses", "spending"],
            entities: &["amount", "merchant", "category"],
            command_count: 12,
            auto_send: false,
        },
        Profile {
            slug: "travel-planner",
            name: "Travel Planner",
            purpose: "Plan itineraries, bookings, and multi-stop trips",
            examples: &[
                "plan a weekend in Jaipur",
                "build an itinerary for Tokyo",
                "compare train and flight options",
                "add the museum to my trip",
            ],
            aliases: &["trip planner", "itinerary"],
            entities: &["place", "date", "booking"],
            command_count: 14,
            auto_send: false,
        },
    ]
}

pub fn ids(limit: usize) -> Vec<String> {
    profiles()
        .iter()
        .take(limit.min(STRESS_COUNT))
        .map(|profile| format!("{LAB_PREFIX}{}", profile.slug))
        .collect()
}

fn project(profile: Profile) -> Result<ExtensionProjectManifest, String> {
    let id = format!("{LAB_PREFIX}{}", profile.slug);
    let auto_send = profile.auto_send.then(|| {
        json!({
            "eligible": true,
            "note": "This lab extension produces a local, reversible result and performs no external side effect."
        })
    });
    let value = json!({
        "id": id,
        "name": profile.name,
        "version": "0.1.0",
        "grainApi": "^1.0",
        "tier": "scripted",
        "kind": "searchable",
        "recommend": {
            "purpose": profile.purpose,
            "examples": profile.examples,
            "aliases": profile.aliases,
            "entities": profile.entities
        },
        "needs": if command_diagnostic(profile.slug) { vec!["semantic"] } else { Vec::<&str>::new() },
        "autoSend": auto_send,
        "description": format!("Recommendation Lab fixture with {} internal commands. No network or external service is used.", profile.command_count),
        "icon": "icon.png",
        "permissions": [],
        "activation": [],
        "entry": "main.js"
    });
    let project: ExtensionProjectManifest =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    let mut validation_manifest = project.manifest.clone();
    validation_manifest.entry_source = LAB_RUNTIME.to_string();
    GrainPack {
        manifest: validation_manifest,
        payloads: PackPayloads::default(),
    }
    .validate_dev()?;
    Ok(project)
}

fn command_diagnostic(slug: &str) -> bool {
    matches!(
        slug,
        "stream-music" | "music-library" | "issue-tracker" | "code-host" | "translator"
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;

    let parent = path.parent().ok_or("lab artifact path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temp = parent.join(format!(".lab-{}.tmp", uuid::Uuid::new_v4().simple()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.to_string());
    }
    drop(file);
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            error.to_string()
        })?;
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        error.to_string()
    })
}

fn materialize_at(root: &Path, icon: &[u8]) -> Result<Vec<LabProject>, String> {
    let parent = root.parent().ok_or("lab root has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    if root.exists() {
        let metadata = std::fs::symlink_metadata(root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("recommendation lab root is not a regular directory".into());
        }
        let marker = std::fs::read_to_string(root.join(LAB_MARKER)).map_err(|_| {
            "refusing to reuse an unmarked Recommendation Lab directory".to_string()
        })?;
        if marker != LAB_MARKER_VALUE {
            return Err(
                "refusing to reuse a Recommendation Lab directory with an unknown marker".into(),
            );
        }
    } else {
        std::fs::create_dir(root).map_err(|error| error.to_string())?;
    }
    let canonical_parent = parent.canonicalize().map_err(|error| error.to_string())?;
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    if canonical_root.parent() != Some(canonical_parent.as_path())
        || canonical_root.file_name().and_then(|name| name.to_str()) != Some(LAB_DIRECTORY)
    {
        return Err("recommendation lab root escaped its fixed data directory".into());
    }

    write_atomic(
        &canonical_root.join(LAB_MARKER),
        LAB_MARKER_VALUE.as_bytes(),
    )?;
    let mut out = Vec::with_capacity(STRESS_COUNT);
    for profile in profiles() {
        let project = project(*profile)?;
        let directory = canonical_root.join(profile.slug);
        if directory.exists() {
            let metadata = std::fs::symlink_metadata(&directory).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(format!(
                    "lab project '{}' is not a regular directory",
                    profile.slug
                ));
            }
        } else {
            std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
        }
        write_atomic(
            &directory.join("manifest.json"),
            &serde_json::to_vec_pretty(&project).map_err(|error| error.to_string())?,
        )?;
        let source = LAB_RUNTIME.replace("__GRAIN_LAB_EXTENSION_ID__", &project.manifest.id);
        write_atomic(&directory.join("main.js"), source.as_bytes())?;
        write_atomic(&directory.join("icon.png"), icon)?;
        out.push(LabProject {
            id: project.manifest.id,
            root: directory,
        });
    }
    Ok(out)
}

pub fn materialize(app: &AppHandle) -> Result<Vec<LabProject>, String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        Err("the Recommendation Lab is available only in debug builds".into())
    }
    #[cfg(debug_assertions)]
    {
        let root = root(app)?;
        materialize_at(&root, include_bytes!("../icons/icon.png"))
    }
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
    fn the_real_dev_loader_accepts_every_lab_project() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(LAB_DIRECTORY);
        let projects = materialize_at(&root, include_bytes!("../icons/icon.png")).unwrap();
        assert_eq!(projects.len(), STRESS_COUNT);
        let mut ids = std::collections::HashSet::new();
        for project in projects {
            let loaded = crate::dev_extensions::load_project(&project.root).unwrap();
            assert_eq!(loaded.pack.manifest.id, project.id);
            assert!(loaded.pack.manifest.kind.is_searchable());
            assert!(loaded.pack.manifest.permissions.is_empty());
            let has_diagnostic = project
                .id
                .strip_prefix(LAB_PREFIX)
                .is_some_and(command_diagnostic);
            assert_eq!(
                loaded.pack.manifest.needs == ["semantic"],
                has_diagnostic,
                "only command-diagnostic fixtures should retain semantic mode"
            );
            assert!(loaded
                .pack
                .manifest
                .entry_source
                .contains("grain.onRequest"));
            assert!(loaded.pack.manifest.entry_source.contains(&project.id));
            assert!(!loaded
                .pack
                .manifest
                .entry_source
                .contains("__GRAIN_LAB_EXTENSION_ID__"));
            assert!(ids.insert(project.id));
        }
        assert_eq!(ids.len(), STRESS_COUNT);
    }

    #[test]
    fn corpus_covers_core_stress_and_runtime_surface_paths() {
        assert_eq!(profiles().len(), STRESS_COUNT);
        assert_eq!(ids(CORE_COUNT).len(), CORE_COUNT);
        assert!(
            profiles()
                .iter()
                .filter(|profile| profile.auto_send)
                .count()
                >= 6
        );
        assert!(profiles().iter().any(|profile| profile.command_count >= 18));
        assert_eq!(LAB_RUNTIME.matches("commandDiagnostic: true").count(), 5);
        for required in [
            "grain.match.lexical",
            "grain.match.semantic",
            "grain.match.decide",
            "grain.ui.onEvent",
            "grain.onRequest",
            "text: \"Suggested\"",
            "text: \"Executed\"",
            "text: \"Auto-send\"",
            "semanticAutoDecision.pick === decision.pick",
            "decline:",
            "error:",
            "kind: \"submit\"",
        ] {
            assert!(
                LAB_RUNTIME.contains(required),
                "missing runtime path {required}"
            );
        }
        for id in ids(STRESS_COUNT) {
            assert!(LAB_RUNTIME.contains(&id), "runtime has no profile for {id}");
        }
    }

    #[test]
    fn an_existing_unmarked_directory_is_never_claimed_or_removed() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(LAB_DIRECTORY);
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("user-file.txt"), b"keep").unwrap();
        assert!(materialize_at(&root, include_bytes!("../icons/icon.png")).is_err());
        assert_eq!(std::fs::read(root.join("user-file.txt")).unwrap(), b"keep");
    }
}
