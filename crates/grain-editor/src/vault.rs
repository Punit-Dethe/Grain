//! [GRAIN] Lightweight vault access for the editor process. Reads the SAME
//! folders of Markdown the main app's store manages, but stays deliberately
//! dumb: no index, no embeddings — a stat-walk per refresh (the Omnisearch
//! lesson: a personal vault rescan is milliseconds). Writes are atomic
//! (tmp + rename); Obsidian merges our disk writes into any open dirty
//! buffer on its side, same as with any external editor.

use std::fs;
use std::path::{Path, PathBuf};

/// Where the editor reads and writes.
#[derive(Clone, Debug)]
pub struct VaultConfig {
    pub root: PathBuf,
    /// Subfolder new notes land in ("" = the root itself). Mirrors the main
    /// app's grain-folder convention so captures and editor notes co-locate.
    pub new_note_folder: String,
    /// Human label for the header ("Obsidian Vault" dir name or "Grain").
    pub label: String,
}

/// One note in the sidebar. `rel` (vault-relative, forward slashes) is the
/// selection key for the session.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteMeta {
    pub title: String,
    pub rel: String,
    pub abs: PathBuf,
    pub pinned: bool,
    /// First-level subfolder = the note's collection; root notes have none.
    pub collection: Option<String>,
    pub mtime_ms: i64,
}

/// Resolve which vault to open, in priority order:
/// 1. an explicit path argument (`grain-editor <vault-dir>`),
/// 2. the main app's `grain.settings.json` (obsidian backend + vault path),
/// 3. the native Grain vault in app data.
pub fn resolve_vault() -> VaultConfig {
    if let Some(arg) = std::env::args().nth(1) {
        let root = PathBuf::from(&arg);
        if root.is_dir() {
            return VaultConfig {
                label: dir_label(&root),
                root,
                new_note_folder: String::new(),
            };
        }
        eprintln!("[grain-editor] not a directory, ignoring arg: {arg}");
    }

    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    if let Some(appdata) = &appdata {
        let settings = appdata.join("com.grain.app").join("grain.settings.json");
        if let Some(cfg) = vault_from_settings(&settings) {
            return cfg;
        }
    }

    let root = appdata
        .map(|d| d.join("com.grain.app").join("grain_space").join("notes"))
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&root);
    VaultConfig {
        root,
        new_note_folder: String::new(),
        label: "Grain".to_string(),
    }
}

/// The main app's settings point at the active vault when the obsidian
/// backend is on — the editor follows them so both UIs see the same notes.
fn vault_from_settings(path: &Path) -> Option<VaultConfig> {
    let text = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    if json.get("grain_space_backend")?.as_str()? != "obsidian" {
        return None;
    }
    let root = PathBuf::from(json.get("grain_space_vault_path")?.as_str()?);
    if !root.is_dir() {
        return None;
    }
    let folder = json
        .get("grain_space_vault_folder")
        .and_then(|v| v.as_str())
        .unwrap_or("Grain")
        .to_string();
    Some(VaultConfig {
        label: dir_label(&root),
        root,
        new_note_folder: folder,
    })
}

fn dir_label(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Vault".to_string())
}

/// Every `.md` under the vault, newest first. Skips dot-dirs (`.obsidian`,
/// `.trash`, VCS) exactly like the main store's reconcile walk.
pub fn scan(root: &Path) -> Vec<NoteMeta> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy().to_string();
            let Ok(ftype) = entry.file_type() else {
                continue;
            };
            if ftype.is_dir() {
                if !name.starts_with('.') && !name.eq_ignore_ascii_case("node_modules") {
                    stack.push(path);
                }
                continue;
            }
            if !ftype.is_file() || !name.to_lowercase().ends_with(".md") {
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let mtime_ms = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let title = Path::new(&rel)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| name.clone());
            let collection = rel.split('/').next().filter(|_| rel.contains('/'));
            out.push(NoteMeta {
                title,
                collection: collection.map(str::to_string),
                pinned: is_pinned(&path),
                abs: path,
                rel,
                mtime_ms,
            });
        }
    }
    out.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));
    out
}

/// Cheap frontmatter peek: a leading `---` block containing `pinned: true`.
/// Only the head of the file is read — the sidebar never loads full bodies.
fn is_pinned(path: &Path) -> bool {
    let Ok(text) = read_head(path, 2048) else {
        return false;
    };
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return false;
    }
    for line in lines {
        let t = line.trim_end();
        if t == "---" || t == "..." {
            return false;
        }
        if t.trim() == "pinned: true" {
            return true;
        }
    }
    false
}

fn read_head(path: &Path, max: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; max];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

pub fn read_text(abs: &Path) -> String {
    fs::read_to_string(abs).unwrap_or_default()
}

/// Atomic save: tmp file in the same directory, rename over the target — the
/// same crash-safety discipline as the main store.
pub fn save_text(abs: &Path, text: &str) {
    let tmp = abs.with_extension("md.tmp");
    if let Err(e) = fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, abs)) {
        eprintln!("[grain-editor] save failed for {}: {e}", abs.display());
    }
}

/// Create `Untitled.md` (collision-suffixed) in the configured new-note
/// folder and return its metadata.
pub fn create_note(cfg: &VaultConfig) -> Option<NoteMeta> {
    let dir = if cfg.new_note_folder.is_empty() {
        cfg.root.clone()
    } else {
        cfg.root.join(&cfg.new_note_folder)
    };
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    for n in 1u32..1000 {
        let name = if n == 1 {
            "Untitled.md".to_string()
        } else {
            format!("Untitled {n}.md")
        };
        let abs = dir.join(&name);
        if abs.exists() {
            continue;
        }
        save_text(&abs, "");
        let rel = abs
            .strip_prefix(&cfg.root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        return Some(NoteMeta {
            title: name.trim_end_matches(".md").to_string(),
            collection: rel
                .split('/')
                .next()
                .filter(|_| rel.contains('/'))
                .map(str::to_string),
            abs,
            rel,
            pinned: false,
            mtime_ms: chrono_now_ms(),
        });
    }
    None
}

fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// "2h ago"-style stamp for sidebar rows. Coarse on purpose.
pub fn rel_age(mtime_ms: i64) -> String {
    let now = chrono_now_ms();
    let mins = ((now - mtime_ms).max(0)) / 60_000;
    match mins {
        0 => "now".to_string(),
        1..=59 => format!("{mins}m"),
        60..=1439 => format!("{}h", mins / 60),
        _ => format!("{}d", mins / 1440),
    }
}
