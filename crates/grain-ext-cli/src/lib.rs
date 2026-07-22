use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use grain_sdk::{
    Contributes, DaemonEvent, ExtensionManifest, ExtensionProjectManifest, GrainPack, PackPayloads,
    ShortcutDecl, Surfaces, Tier, GRAIN_API_TYPESCRIPT, GRAIN_API_VERSION, KNOWN_CAPABILITIES,
};
use specta::TypeCollection;
use specta_typescript::{BigIntExportBehavior, Typescript};

const HELP: &str = "grain-ext — build Grain extensions

Usage:
  grain-ext init <name> [--id <reverse-dns-id>]
  grain-ext dev [--token-file <path>]
  grain-ext doctor
  grain-ext --help
  grain-ext --version";

#[derive(Debug)]
pub struct InitResult {
    pub root: PathBuf,
    pub output: String,
}

/// Run the CLI against an explicit working directory. Keeping argument parsing
/// free of process globals makes every command deterministic and testable.
pub fn run<I>(args: I, cwd: &Path) -> Result<String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(HELP.into());
    };

    match command.as_str() {
        "--help" | "-h" | "help" => Ok(HELP.into()),
        "--version" | "-V" => Ok(format!("grain-ext {}", env!("CARGO_PKG_VERSION"))),
        "init" => {
            let name = args.next().context("init requires an extension name")?;
            if name.starts_with('-') {
                bail!("init requires an extension name before its options");
            }

            let mut id = None;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--id" => {
                        if id.is_some() {
                            bail!("--id may be supplied only once");
                        }
                        id = Some(args.next().context("--id requires a value")?);
                    }
                    _ => bail!("unknown init option '{flag}'"),
                }
            }

            Ok(init_project(cwd, &name, id.as_deref())?.output)
        }
        "dev" => {
            let mut token_file = None;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--token-file" => {
                        if token_file.is_some() {
                            bail!("--token-file may be supplied only once");
                        }
                        token_file = Some(PathBuf::from(
                            args.next().context("--token-file requires a path")?,
                        ));
                    }
                    _ => bail!("unknown dev option '{flag}'"),
                }
            }
            dev_project(cwd, token_file.as_deref())?;
            Ok("development watcher stopped".into())
        }
        "doctor" => {
            if let Some(argument) = args.next() {
                bail!("unknown doctor option '{argument}'");
            }
            let report = grain_extension_checks::doctor(cwd);
            if report.is_clean() {
                Ok(report.to_string())
            } else {
                bail!(report.to_string())
            }
        }
        _ => bail!("unknown command '{command}'\n\n{HELP}"),
    }
}

/// Create a scripted extension project without overwriting an existing path.
/// If any write fails, the just-created directory is removed as one unit so a
/// failed scaffold never looks complete.
pub fn init_project(cwd: &Path, name: &str, id: Option<&str>) -> Result<InitResult> {
    let name = name.trim();
    if name.is_empty() {
        bail!("extension name must not be empty");
    }
    let slug = slugify(name)?;
    let id = id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("com.example.{slug}"));
    let root = cwd.join(&slug);

    fs::create_dir(&root)
        .with_context(|| format!("create project directory {}", root.display()))?;
    let mut guard = NewProjectGuard::new(root.clone());
    fs::create_dir(root.join("src")).context("create source directory")?;

    let project = scaffold_manifest(name, &id);
    validate_scaffold(&project)?;
    write_json(&root.join("manifest.json"), &project)?;
    write_text(&root.join("src/main.ts"), &entry_source(name)?)?;
    write_text(&root.join("grain.d.ts"), &typescript_declarations()?)?;
    write_json(&root.join("package.json"), &package_json(&slug))?;
    write_json(&root.join("tsconfig.json"), &tsconfig_json())?;
    write_text(&root.join("README.md"), &readme(name, &id))?;
    write_text(
        &root.join(".gitignore"),
        "node_modules/\ndist/\n*.grainpack\n",
    )?;

    guard.keep();
    Ok(InitResult {
        root: root.clone(),
        output: format!(
            "Created {}\n\nScripted extensions use Node.js and esbuild for bundling.\n\nNext:\n  cd {}\n  npm install\n  npm run build\n  grain-ext doctor\n  Add this folder in Grain > Extensions > Developer mode\n  grain-ext dev",
            root.display(),
            slug
        ),
    })
}

fn slugify(name: &str) -> Result<String> {
    let mut slug = String::with_capacity(name.len());
    let mut separator = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        bail!("extension name must contain at least one ASCII letter or digit");
    }
    Ok(slug)
}

fn scaffold_manifest(name: &str, id: &str) -> ExtensionProjectManifest {
    ExtensionProjectManifest {
        manifest: ExtensionManifest {
            id: id.into(),
            name: name.into(),
            version: "0.1.0".into(),
            grain_api: format!("^{GRAIN_API_VERSION}"),
            tier: Tier::Scripted,
            description: format!("A Grain extension by {name}"),
            repository: None,
            permissions: Vec::new(),
            activation: vec!["onShortcut:open".into()],
            entry_source: String::new(),
            surfaces: Surfaces::default(),
            slots: Vec::new(),
            contributes: Contributes {
                settings: Vec::new(),
                shortcuts: vec![ShortcutDecl {
                    id: "open".into(),
                    label: format!("Open {name}"),
                    default_binding: Some("Ctrl+Alt+Shift+G".into()),
                }],
                session_mode: None,
            },
            companion: None,
        },
        entry: "dist/main.js".into(),
    }
}

#[derive(serde::Deserialize)]
struct DevTokenFile {
    url: String,
    token: String,
}

struct DevClient {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    next_request: u64,
}

impl DevClient {
    fn connect(file: &Path) -> Result<Self> {
        let raw = fs::read_to_string(file).with_context(|| {
            format!(
                "read developer token {}; enable Developer mode in Grain first",
                file.display()
            )
        })?;
        let config: DevTokenFile = serde_json::from_str(&raw).context("parse developer token")?;
        let (mut socket, _) = tungstenite::connect(config.url.as_str())
            .context("connect to Grain developer channel")?;
        socket.send(tungstenite::Message::Text(
            serde_json::to_string(&grain_sdk::ClientHello {
                token: config.token,
                client: "grain-ext".into(),
                grain_api: GRAIN_API_VERSION.into(),
            })?
            .into(),
        ))?;
        let tungstenite::Message::Text(welcome) = socket.read()? else {
            bail!("Grain returned an invalid developer handshake");
        };
        serde_json::from_str::<grain_sdk::ServerWelcome>(&welcome)
            .context("Grain rejected the developer token")?;
        Ok(Self {
            socket,
            next_request: 1,
        })
    }

    fn reload(&mut self, extension_id: &str) -> Result<grain_sdk::DevReloadResult> {
        let request_id = self.next_request;
        self.next_request += 1;
        self.socket.send(tungstenite::Message::Text(
            serde_json::to_string(&grain_sdk::DevControlFrame::DevReload {
                request_id,
                extension_id: extension_id.into(),
            })?
            .into(),
        ))?;
        loop {
            let tungstenite::Message::Text(raw) = self.socket.read()? else {
                continue;
            };
            let Ok(grain_sdk::DevControlFrame::DevResult {
                request_id: response_id,
                result,
                error,
            }) = serde_json::from_str(&raw)
            else {
                continue;
            };
            if response_id != request_id {
                continue;
            }
            if let Some(error) = error {
                bail!(error);
            }
            return result.context("Grain returned an empty reload result");
        }
    }
}

fn reload_with_reconnect(
    client: &mut DevClient,
    token_file: &Path,
    extension_id: &str,
) -> Result<grain_sdk::DevReloadResult> {
    match client.reload(extension_id) {
        Ok(result) => Ok(result),
        Err(first_error) => {
            *client = DevClient::connect(token_file).with_context(|| first_error.to_string())?;
            client.reload(extension_id)
        }
    }
}

fn default_token_file() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("GRAIN_APP_DATA_DIR") {
        return Ok(PathBuf::from(path).join("extension-dev-token.json"));
    }
    #[cfg(target_os = "windows")]
    let base = PathBuf::from(std::env::var_os("APPDATA").context("APPDATA is not set")?);
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
        .join("Library/Application Support");
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    Ok(base.join("com.grain.app").join("extension-dev-token.json"))
}

fn build_project(root: &Path) -> Result<()> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = std::process::Command::new(npm)
        .args(["run", "build"])
        .current_dir(root)
        .status()
        .context("run npm build")?;
    if !status.success() {
        bail!("npm build failed");
    }
    Ok(())
}

struct BuildWatcher(std::process::Child);

impl BuildWatcher {
    fn start(root: &Path) -> Result<Self> {
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
        let child = std::process::Command::new(npm)
            .args(["run", "build", "--", "--watch=forever"])
            .current_dir(root)
            .spawn()
            .context("start incremental npm build")?;
        Ok(Self(child))
    }
}

impl Drop for BuildWatcher {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &self.0.id().to_string()])
            .output();
        #[cfg(not(windows))]
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn ignored_watch_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .any(|component| {
            matches!(
                component.as_os_str().to_str(),
                Some("node_modules" | ".git")
            )
        })
}

fn reload_watch_path(root: &Path, path: &Path, entry: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative == Path::new("manifest.json") || relative == entry
}

/// Build once, reload, then rebuild and hot-reload on source changes.
pub fn dev_project(root: &Path, token_file: Option<&Path>) -> Result<()> {
    use notify::Watcher;

    let manifest_path = root.join("manifest.json");
    let read_project = || -> Result<ExtensionProjectManifest> {
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?;
        serde_json::from_str(&raw).context("parse manifest.json")
    };
    let token_file = token_file
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(default_token_file)?;
    let started = Instant::now();
    build_project(root)?;
    let mut client = DevClient::connect(&token_file)?;
    let project = read_project()?;
    let result = reload_with_reconnect(&mut client, &token_file, &project.manifest.id)?;
    println!(
        "Reloaded in {} ms (workers {}, tokens {})",
        started.elapsed().as_millis(),
        result.worker_count,
        result.token_count
    );

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })?;
    watcher.watch(root, notify::RecursiveMode::Recursive)?;
    let mut build_watcher = BuildWatcher::start(root)?;
    println!("Watching {}", root.display());
    loop {
        if let Some(status) = build_watcher.0.try_wait()? {
            bail!("incremental npm build stopped ({status})");
        }
        let event: notify::Result<notify::Event> = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => event,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => bail!("file watcher stopped"),
        };
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                eprintln!("grain-ext: watch error: {error}");
                continue;
            }
        };
        if event
            .paths
            .iter()
            .all(|path| ignored_watch_path(root, path))
        {
            continue;
        }
        let project = match read_project() {
            Ok(project) => project,
            Err(error) => {
                eprintln!("grain-ext: {error:#}");
                continue;
            }
        };
        let entry = Path::new(&project.entry);
        let mut should_reload = event
            .paths
            .iter()
            .any(|path| reload_watch_path(root, path, entry));
        while let Ok(next) = rx.recv_timeout(Duration::from_millis(40)) {
            if let Ok(next) = next {
                should_reload |= next
                    .paths
                    .iter()
                    .any(|path| reload_watch_path(root, path, entry));
            }
        }
        if !should_reload {
            continue;
        }
        let started = Instant::now();
        let reload = reload_with_reconnect(&mut client, &token_file, &project.manifest.id);
        match reload {
            Ok(result) => println!(
                "Reloaded in {} ms (workers {}, tokens {})",
                started.elapsed().as_millis(),
                result.worker_count,
                result.token_count
            ),
            Err(error) => eprintln!("grain-ext: {error:#}"),
        }
    }
}

fn validate_scaffold(project: &ExtensionProjectManifest) -> Result<()> {
    let mut manifest = project.manifest.clone();
    manifest.entry_source = "// built by grain-ext".into();
    GrainPack {
        manifest,
        payloads: PackPayloads::default(),
    }
    .validate()
    .map_err(anyhow::Error::msg)
    .context("generated manifest is invalid")?;

    let entry = Path::new(&project.entry);
    if entry.is_absolute()
        || entry
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        bail!("manifest entry must stay inside the project");
    }
    Ok(())
}

fn typescript_declarations() -> Result<String> {
    let mut types = TypeCollection::default();
    types.register::<DaemonEvent>();
    let reflected = Typescript::new()
        .framework_header("// Generated from grain-sdk by grain-ext. DO NOT EDIT.")
        .bigint(BigIntExportBehavior::Number)
        .export(&types)
        .context("generate Grain event types")?;
    let capabilities = KNOWN_CAPABILITIES
        .iter()
        .map(|cap| format!("  | {cap:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "{reflected}\nexport type GrainCapability =\n{capabilities};\n\n{GRAIN_API_TYPESCRIPT}\n"
    ))
}

fn entry_source(name: &str) -> Result<String> {
    let name = serde_json::to_string(name)?;
    Ok(format!(
        "const extensionName = {name};\n\ngrain.log.info(`${{extensionName}} loaded`);\n\ngrain.onShortcut(async (id) => {{\n  if (id === \"open\") {{\n    await grain.log.info(`${{extensionName}} shortcut pressed`);\n  }}\n}});\n"
    ))
}

fn package_json(slug: &str) -> serde_json::Value {
    serde_json::json!({
        "name": slug,
        "version": "0.1.0",
        "private": true,
        "scripts": {
            "build": "esbuild src/main.ts --bundle --format=iife --platform=browser --target=es2020 --outfile=dist/main.js --sourcemap"
        },
        "devDependencies": {
            "esbuild": "^0.25.0",
            "typescript": "^5.8.0"
        }
    })
}

fn tsconfig_json() -> serde_json::Value {
    serde_json::json!({
        "compilerOptions": {
            "target": "ES2020",
            "module": "ESNext",
            "moduleResolution": "Bundler",
            "strict": true,
            "noEmit": true,
            "lib": ["ES2020", "WebWorker"]
        },
        "include": ["grain.d.ts", "src/**/*.ts"]
    })
}

fn readme(name: &str, id: &str) -> String {
    format!(
        "# {name}\n\nGrain extension id: `{id}`\n\n## Develop\n\n1. Install Node.js and run `npm install`.\n2. Run `npm run build` once.\n3. Run `grain-ext doctor`.\n4. Enable Developer mode in Grain and add this folder as an unpacked extension.\n5. Run `grain-ext dev` for incremental builds and hot reload.\n\nEdit `src/main.ts`; `grain.d.ts` is generated from the Grain SDK.\n"
    )
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    write_text(path, &json)
}

fn write_text(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

struct NewProjectGuard {
    root: PathBuf,
    keep: bool,
}

impl NewProjectGuard {
    fn new(root: PathBuf) -> Self {
        Self { root, keep: false }
    }

    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for NewProjectGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_a_valid_typed_scripted_project() {
        let temp = tempfile::tempdir().unwrap();
        let result = init_project(temp.path(), "Focus Notes", None).unwrap();
        assert_eq!(result.root, temp.path().join("focus-notes"));

        for file in [
            "manifest.json",
            "src/main.ts",
            "grain.d.ts",
            "package.json",
            "tsconfig.json",
            "README.md",
            ".gitignore",
        ] {
            assert!(result.root.join(file).is_file(), "missing {file}");
        }

        let raw = fs::read_to_string(result.root.join("manifest.json")).unwrap();
        let project: ExtensionProjectManifest = serde_json::from_str(&raw).unwrap();
        assert_eq!(project.manifest.id, "com.example.focus-notes");
        assert_eq!(project.manifest.grain_api, "^1.0");
        assert_eq!(project.entry, "dist/main.js");
        assert_eq!(
            project.manifest.contributes.shortcuts[0]
                .default_binding
                .as_deref(),
            Some("Ctrl+Alt+Shift+G")
        );
        validate_scaffold(&project).unwrap();

        let declarations = fs::read_to_string(result.root.join("grain.d.ts")).unwrap();
        assert!(declarations.contains("export type DaemonEvent"));
        assert!(declarations.contains("interface GrainError extends Error"));
        assert!(declarations
            .split_once("declare global")
            .is_some_and(|(_, global)| global.contains("interface GrainError extends Error")));
        assert!(declarations.contains("E_CAPABILITY_DENIED"));
        assert!(declarations.contains("const grain: GrainApi"));
        for capability in KNOWN_CAPABILITIES {
            assert!(declarations.contains(capability), "missing {capability}");
        }
    }

    #[test]
    fn init_refuses_to_overwrite_an_existing_project() {
        let temp = tempfile::tempdir().unwrap();
        let first = init_project(temp.path(), "Focus Notes", None).unwrap();
        let marker = first.root.join("README.md");
        fs::write(&marker, "keep me").unwrap();

        assert!(init_project(temp.path(), "Focus Notes", None).is_err());
        assert_eq!(fs::read_to_string(marker).unwrap(), "keep me");
    }

    #[test]
    fn cli_accepts_a_custom_id_and_explains_the_toolchain() {
        let temp = tempfile::tempdir().unwrap();
        let output = run(
            [
                "init".into(),
                "My Tool".into(),
                "--id".into(),
                "dev.example.my-tool".into(),
            ],
            temp.path(),
        )
        .unwrap();
        assert!(output.contains("Node.js and esbuild"));
        assert!(output.contains("grain-ext dev"));

        let raw = fs::read_to_string(temp.path().join("my-tool/manifest.json")).unwrap();
        let project: ExtensionProjectManifest = serde_json::from_str(&raw).unwrap();
        assert_eq!(project.manifest.id, "dev.example.my-tool");
    }

    #[test]
    fn doctor_accepts_a_fresh_unbuilt_scaffold() {
        let temp = tempfile::tempdir().unwrap();
        let project = init_project(temp.path(), "Doctor Test", None).unwrap();
        let output = run(["doctor".into()], &project.root).unwrap();
        assert!(output.starts_with("doctor: 0 findings"));
    }

    #[test]
    fn doctor_failure_preserves_precise_unicode_diagnostic() {
        let temp = tempfile::tempdir().unwrap();
        let project = init_project(temp.path(), "Doctor Test", None).unwrap();
        fs::write(
            project.root.join("src/main.ts"),
            "const visible = true;\nconst hidden = '\u{200b}';\n",
        )
        .unwrap();

        let error = run(["doctor".into()], &project.root).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("src/main.ts:2:17"));
        assert!(message.contains("U+200B"));
    }

    #[test]
    fn watcher_ignores_generated_and_dependency_trees() {
        let root = Path::new("project");
        assert!(!ignored_watch_path(root, &root.join("dist/main.js")));
        assert!(ignored_watch_path(
            root,
            &root.join("node_modules/pkg/index.js")
        ));
        assert!(ignored_watch_path(root, &root.join(".git/index")));
        assert!(!ignored_watch_path(root, &root.join("src/main.ts")));
        assert!(!ignored_watch_path(root, &root.join("manifest.json")));
        let entry = Path::new("dist/main.js");
        assert!(reload_watch_path(root, &root.join("dist/main.js"), entry));
        assert!(reload_watch_path(root, &root.join("manifest.json"), entry));
        assert!(!reload_watch_path(
            root,
            &root.join("dist/main.js.map"),
            entry
        ));
        assert!(!reload_watch_path(root, &root.join("src/main.ts"), entry));
    }
}
