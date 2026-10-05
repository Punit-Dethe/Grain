use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use grain_sdk::manifest::Surfaces;
use grain_sdk::{
    Contributes, ExtensionManifest, ExtensionProjectManifest, GrainPack, PackPayloads, Tier,
    GRAIN_API_TYPESCRIPT, GRAIN_API_VERSION, KNOWN_CAPABILITIES,
};

const HELP: &str = "grain-ext — build Grain extensions

Usage:
  grain-ext init <name> [--id <reverse-dns-id>]
  grain-ext init <name> --mcp-url <https-url> [--authentication oauth|none]
                 [--id <reverse-dns-id>]
  grain-ext dev [--token-file <path>]
  grain-ext doctor
  grain-ext pack [--output <path>]
  grain-ext submit --registry <path> --repo <url> --tag <tag> --commit <sha>
                   --contact <maintainer> [--license <spdx>] [--category tools]
  grain-ext --help
  grain-ext --version";
static NEXT_ARTIFACT_TEMP: AtomicU64 = AtomicU64::new(1);

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
            let mut mcp_url = None;
            let mut authentication = None;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--id" => {
                        if id.is_some() {
                            bail!("--id may be supplied only once");
                        }
                        id = Some(args.next().context("--id requires a value")?);
                    }
                    "--mcp-url" => {
                        if mcp_url.is_some() {
                            bail!("--mcp-url may be supplied only once");
                        }
                        mcp_url = Some(args.next().context("--mcp-url requires a value")?);
                    }
                    "--authentication" => {
                        if authentication.is_some() {
                            bail!("--authentication may be supplied only once");
                        }
                        authentication = Some(
                            args.next()
                                .context("--authentication requires oauth or none")?,
                        );
                    }
                    _ => bail!("unknown init option '{flag}'"),
                }
            }

            if let Some(url) = mcp_url {
                Ok(init_mcp_project(
                    cwd,
                    &name,
                    id.as_deref(),
                    &url,
                    authentication.as_deref().unwrap_or("oauth"),
                )?
                .output)
            } else {
                if authentication.is_some() {
                    bail!("--authentication requires --mcp-url");
                }
                Ok(init_project(cwd, &name, id.as_deref())?.output)
            }
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
        "pack" => pack_project(cwd, args),
        "submit" => submit_project(cwd, args),
        _ => bail!("unknown command '{command}'\n\n{HELP}"),
    }
}

/// Compile (when scripted), run the same checks as registry CI, and emit the
/// single-file artifact Grain installs. The verified 512² icon master is
/// embedded, so an installed pack never depends on the author's source tree.
fn pack_project<I>(cwd: &Path, mut args: I) -> Result<String>
where
    I: Iterator<Item = String>,
{
    let mut output: Option<PathBuf> = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--output" => {
                if output.is_some() {
                    bail!("--output may be supplied only once");
                }
                output = Some(PathBuf::from(
                    args.next().context("--output requires a path")?,
                ));
            }
            other => bail!("unknown pack option '{other}'"),
        }
    }

    if grain_extension_checks::is_mcp_project(cwd).map_err(anyhow::Error::msg)? {
        let report = grain_extension_checks::doctor(cwd);
        if !report.is_clean() {
            bail!("doctor found problems:\n{report}");
        }
        let descriptor =
            grain_extension_checks::read_mcp_descriptor(cwd).map_err(anyhow::Error::msg)?;
        let bytes = serde_json::to_vec(&descriptor).context("serialize MCP descriptor")?;
        return publish_artifact(
            cwd,
            output,
            format!("{}-{}.mcp.json", descriptor.id, descriptor.version),
            &bytes,
        );
    }
    let raw = fs::read_to_string(cwd.join("manifest.json"))
        .context("read manifest.json (run pack from the project root)")?;
    let project: ExtensionProjectManifest =
        serde_json::from_str(&raw).context("parse manifest.json")?;
    if project.manifest.tier == Tier::Scripted {
        build_project(cwd)?;
    }
    let pack = grain_extension_checks::build_pack(cwd).map_err(anyhow::Error::msg)?;
    let default_name = format!("{}-{}.grainpack", pack.manifest.id, pack.manifest.version);
    let bytes = serde_json::to_vec(&pack).context("serialize .grainpack")?;
    publish_artifact(cwd, output, default_name, &bytes)
}

fn publish_artifact(
    cwd: &Path,
    output: Option<PathBuf>,
    default_name: String,
    bytes: &[u8],
) -> Result<String> {
    let output = output.unwrap_or_else(|| PathBuf::from(default_name));
    let output = if output.is_absolute() {
        output
    } else {
        cwd.join(output)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }
    // Neither source definition may become a built output. Creating the other
    // definition would make a previously valid project ambiguous as well.
    let root = cwd.canonicalize().context("resolve project root")?;
    let output_parent = output.parent().and_then(|p| p.canonicalize().ok());
    for source in ["mcp.json", "manifest.json"] {
        let reserved = output_parent.as_ref() == Some(&root)
            && output
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(source));
        let alias = output
            .canonicalize()
            .ok()
            .is_some_and(|p| Some(p) == root.join(source).canonicalize().ok());
        if reserved || alias {
            bail!("artifact output must not replace {source}");
        }
    }
    replace_artifact(&output, bytes)?;
    Ok(format!("Built {}", output.display()))
}

/// Metadata-only MCP authoring. No Node project, worker, credentials or local
/// server command is generated. OAuth discovery/consent belongs to Grain.
pub fn init_mcp_project(
    cwd: &Path,
    name: &str,
    id: Option<&str>,
    url: &str,
    authentication: &str,
) -> Result<InitResult> {
    use grain_sdk::mcp::{McpAuthentication, McpDescriptor, McpTransport, MCP_DESCRIPTOR_SCHEMA};
    let name = name.trim();
    let slug = slugify(name)?;
    let id = id.unwrap_or("").trim();
    let descriptor = McpDescriptor {
        schema: MCP_DESCRIPTOR_SCHEMA,
        id: if id.is_empty() {
            format!("com.example.{slug}")
        } else {
            id.into()
        },
        name: name.into(),
        description: format!("Tools provided by {name}"),
        version: "0.1.0".into(),
        grain_api: grain_sdk::compatibility::EXTENSION_API_REQUIREMENT.into(),
        transport: McpTransport::StreamableHttp { url: url.into() },
        authentication: match authentication {
            "oauth" => McpAuthentication::OAuth {},
            "none" => McpAuthentication::None {},
            _ => bail!("--authentication must be oauth or none"),
        },
    };
    let root = cwd.join(&slug);
    fs::create_dir(&root)
        .with_context(|| format!("create project directory {}", root.display()))?;
    let mut guard = NewProjectGuard::new(root.clone());
    write_json(&root.join("mcp.json"), &descriptor)?;
    // Canonicalize with the same validator the host uses before keeping any project.
    let descriptor =
        grain_extension_checks::read_mcp_descriptor(&root).map_err(anyhow::Error::msg)?;
    write_json(&root.join("mcp.json"), &descriptor)?;
    write_text(&root.join("README.md"), &format!("# {name}\n\nMCP extension `{}`. Edit `mcp.json`, run `grain-ext doctor`, then `grain-ext pack`.\n\nGrain owns connection identities, enablement and OAuth consent. Never put credentials, client IDs, headers or server-launch commands in this descriptor. No Grain JavaScript SDK or Node build is required. `grain-ext dev` applies only to native tool projects. Store publishing remains provisional.\n", descriptor.id))?;
    write_text(
        &root.join("DESCRIPTION.md"),
        &format!("# {name}\n\n{}\n", descriptor.description),
    )?;
    write_text(&root.join(".gitignore"), "*.mcp.json\n")?;
    guard.keep();
    Ok(InitResult { root: root.clone(), output: format!("Created {}\n\nMCP descriptor only; no Node build or server launch. OAuth is managed by Grain.\n\nNext:\n  cd {slug}\n  grain-ext doctor\n  grain-ext pack\n\nStore submission and listing remain provisional.", root.display()) })
}

fn replace_artifact(output: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;

    let parent = output.parent().context("artifact path has no parent")?;
    let sequence = NEXT_ARTIFACT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("artifact name is not valid Unicode")?;
    let temp = parent.join(format!(".{name}.{}-{sequence}.tmp", std::process::id()));
    let backup = parent.join(format!(
        ".{name}.{}-{sequence}.previous",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .with_context(|| format!("create {}", temp.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| format!("write {}", temp.display()));
    }
    drop(file);

    let had_output = output.exists();
    if had_output {
        fs::rename(output, &backup)
            .with_context(|| format!("prepare replacement for {}", output.display()))?;
    }
    if let Err(error) = fs::rename(&temp, output) {
        let _ = fs::remove_file(&temp);
        if had_output {
            let _ = fs::rename(&backup, output);
        }
        return Err(error).with_context(|| format!("finish {}", output.display()));
    }
    if had_output {
        let _ = fs::remove_file(backup);
    }
    Ok(())
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
        &root.join("DESCRIPTION.md"),
        &format!("# {name}\n\nA native Grain tool extension.\n"),
    )?;
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

/// `grain-ext submit` — write `extensions/<id>/submission.toml` + DESCRIPTION into a
/// local checkout of the `grain-extensions` registry and print the next steps
/// (the author opens the PR). Runs `doctor` first: a submission that fails the
/// checks should never be opened.
fn submit_project<I>(cwd: &Path, mut args: I) -> Result<String>
where
    I: Iterator<Item = String>,
{
    let mut registry: Option<PathBuf> = None;
    let mut repo: Option<String> = None;
    let mut tag: Option<String> = None;
    let mut commit: Option<String> = None;
    let mut license = "MIT".to_string();
    let mut contact: Option<String> = None;
    let mut categories: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    while let Some(flag) = args.next() {
        if flag != "--category" && !seen.insert(flag.clone()) {
            bail!("{flag} may be supplied only once");
        }
        let mut val = || {
            args.next()
                .with_context(|| format!("{flag} requires a value"))
        };
        match flag.as_str() {
            "--registry" => registry = Some(PathBuf::from(val()?)),
            "--repo" => repo = Some(val()?),
            "--tag" => tag = Some(val()?),
            "--commit" => commit = Some(val()?),
            "--license" => license = val()?,
            "--contact" => contact = Some(val()?),
            "--category" => categories.push(val()?),
            other => bail!("unknown submit option '{other}'"),
        }
    }

    let registry = registry.context("--registry <path to grain-extensions> is required")?;
    let repo = repo.context("--repo <source repo url> is required")?;
    let tag = tag.context("--tag <pinned tag> is required")?;
    let commit = commit.context("--commit <pinned commit sha> is required")?;

    // Doctor gate — the exact CI checks, run locally, before a PR is opened.
    let report = grain_extension_checks::doctor(cwd);
    if !report.is_clean() {
        bail!("doctor found problems — fix these before submitting:\n{report}");
    }

    let project = grain_extension_checks::project_identity(cwd).map_err(anyhow::Error::msg)?;
    let listing = grain_extension_checks::listing::read_listing(cwd).map_err(anyhow::Error::msg)?;
    let id = project.id.clone();
    if categories.is_empty() {
        categories.push("tools".into());
    }
    let submission = grain_sdk::submission::SourceSubmission {
        schema: grain_sdk::submission::SUBMISSION_SCHEMA,
        artifact_kind: project.kind,
        id: id.clone(),
        version: project.version,
        grain_api: project.grain_api,
        source_repo: repo,
        tag,
        commit,
        summary: project.summary,
        categories,
        license,
        contact: contact.context("--contact <maintainer contact> is required")?,
        description_sha256: listing.sha256,
        description_size: listing.description.len() as u64,
        media: listing.media,
    };
    let toml =
        grain_extension_checks::serialize_submission(&submission).map_err(anyhow::Error::msg)?;
    let registry = registry
        .canonicalize()
        .context("registry checkout must exist")?;
    let extensions = registry.join("extensions");
    if extensions.try_exists()? {
        if fs::symlink_metadata(&extensions)?.file_type().is_symlink() || !extensions.is_dir() {
            bail!("registry extensions path must be a real directory");
        }
    } else {
        fs::create_dir(&extensions)?;
    }
    if !extensions.canonicalize()?.starts_with(&registry) {
        bail!("registry extensions path escapes its checkout");
    }
    let dir = extensions.join(&id);
    fs::create_dir(&dir).with_context(|| {
        format!(
            "create new submission {}; existing submissions are never overwritten",
            dir.display()
        )
    })?;
    let mut guard = NewProjectGuard::new(dir.clone());
    write_text(&dir.join("DESCRIPTION.md"), &listing.description)?;
    // Publish the completion marker last. Maintainer checks require both its
    // strict schema and the matching description; partial output cannot pass.
    replace_artifact(&dir.join("submission.toml"), toml.as_bytes())?;
    guard.keep();

    Ok(format!(
        "Wrote {}\n\nProvisional source submission only; no build, upload, signing or release performed. E4 publishing migration is required.\n\nNext:\n  cd {}\n  git add extensions/{id}\n  git commit -m \"add {id}\"\n  git push and open a pull request against grain-extensions",
        dir.join("submission.toml").display(),
        registry.display(),
    ))
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
            // Tool discovery uses declared actions; no whole-request opt-in.
            kind: Default::default(),
            recommend: None,
            auto_send: None,
            needs: Vec::new(),
            description: format!("A Grain extension by {name}"),
            // [GRAIN] Point at where the icon belongs so `doctor` guides the
            // author to drop in a 512×512 icon.png rather than leaving them to
            // discover the field. An icon is required to submit (§13.3); the
            // scaffold cannot invent art, so `doctor` reports the missing file
            // until the author adds it.
            icon: "icon.png".into(),
            repository: None,
            permissions: Vec::new(),
            activation: Vec::new(),
            entry_source: String::new(),
            surfaces: Surfaces::default(),
            slots: Vec::new(),
            variant_slots: Vec::new(),
            contributes: Contributes {
                settings: Vec::new(),
                actions: vec![serde_json::from_value(serde_json::json!({
                    "id": "hello", "title": "Say hello", "risk": "safe", "utterances": ["say hello"]
                }))
                .expect("static tool declaration")],
                session_mode: None,
                // The scaffold declares neither authentication nor session
                // mode; both require an explicit author decision and consent.
                ..Default::default()
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

    if grain_extension_checks::is_mcp_project(root).map_err(anyhow::Error::msg)? {
        bail!("MCP projects have no Grain worker to reload; edit mcp.json, then run grain-ext doctor and grain-ext pack. Configure your server through Grain's MCP connection flow.");
    }

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
    let capabilities = KNOWN_CAPABILITIES
        .iter()
        .filter(|cap| grain_sdk::manifest::tool_permission_allowed(cap))
        .map(|cap| format!("  | {cap:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "// Generated public tool API from grain-sdk by grain-ext. DO NOT EDIT.\nexport type GrainCapability =\n{capabilities}\n  | `net:${{string}}`;\n\n{GRAIN_API_TYPESCRIPT}\n"
    ))
}

fn entry_source(name: &str) -> Result<String> {
    let name = serde_json::to_string(name)?;
    Ok(format!(
        "const extensionName = {name};\n\ngrain.actions({{\n  hello: async () => ({{ ok: {{ title: extensionName, body: \"Hello from this tool.\" }} }})\n}});\n"
    ))
}

fn package_json(slug: &str) -> serde_json::Value {
    serde_json::json!({
        "name": slug,
        "version": "0.1.0",
        "private": true,
        "scripts": {
            "check": "tsc --noEmit",
            "build": "tsc --noEmit && esbuild src/main.ts --bundle --format=iife --platform=browser --target=es2020 --outfile=dist/main.js --sourcemap"
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
            "exactOptionalPropertyTypes": true,
            "noEmit": true,
            "lib": ["ES2020", "WebWorker"]
        },
        "include": ["grain.d.ts", "src/**/*.ts"]
    })
}

fn readme(name: &str, id: &str) -> String {
    format!(
        "# {name}\n\nGrain extension id: `{id}`\n\n## Develop\n\n1. Install Node.js and run `npm install`.\n2. Run `npm run build` once.\n3. Run `grain-ext doctor`.\n4. Enable Developer mode in Grain and add this folder as an unpacked extension.\n5. Run `grain-ext dev` for incremental builds and hot reload.\n\nEdit `src/main.ts`; `grain.d.ts` is generated from the Grain SDK. `DESCRIPTION.md` is reserved for user-facing listing text; README is developer documentation. Catalog/media migration remains provisional.\n"
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

    fn submission_args(registry: &Path) -> Vec<String> {
        vec![
            "submit".into(),
            "--registry".into(),
            registry.display().to_string(),
            "--repo".into(),
            "https://github.com/example/remote-tools".into(),
            "--tag".into(),
            "v1.0.0".into(),
            "--commit".into(),
            "1234567890abcdef1234567890abcdef12345678".into(),
            "--contact".into(),
            "Maintainer \"quoted\" \\ contact".into(),
        ]
    }

    #[test]
    fn mcp_submission_round_trips_quotes_and_refuses_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let project = init_mcp_project(
            root.path(),
            "Remote tools",
            None,
            "https://tools.example/mcp",
            "oauth",
        )
        .unwrap();
        let registry = tempfile::tempdir().unwrap();
        let args = submission_args(registry.path());
        let output = run(args.clone(), &project.root).unwrap();
        assert!(output.contains("no build, upload, signing or release"));
        let folder = registry.path().join("extensions/com.example.remote-tools");
        let parsed =
            grain_extension_checks::read_submission(&folder, "com.example.remote-tools").unwrap();
        assert_eq!(
            parsed.artifact_kind,
            grain_sdk::distribution::ArtifactKind::McpDescriptor
        );
        assert_eq!(parsed.categories, ["tools"]);
        assert_eq!(parsed.contact, "Maintainer \"quoted\" \\ contact");
        assert!(!folder.join("README.md").exists());
        let previous = fs::read(folder.join("submission.toml")).unwrap();
        assert!(run(args, &project.root)
            .unwrap_err()
            .to_string()
            .contains("existing submissions"));
        assert_eq!(fs::read(folder.join("submission.toml")).unwrap(), previous);
        fs::write(folder.join("DESCRIPTION.md"), "changed after snapshot").unwrap();
        assert!(
            grain_extension_checks::read_submission(&folder, "com.example.remote-tools")
                .unwrap_err()
                .contains("differs")
        );
    }

    #[test]
    fn invalid_submission_inputs_and_missing_description_publish_nothing() {
        let root = tempfile::tempdir().unwrap();
        let project = init_mcp_project(
            root.path(),
            "Remote tools",
            None,
            "https://tools.example/mcp",
            "none",
        )
        .unwrap();
        let registry = tempfile::tempdir().unwrap();
        for (flag, value) in [
            ("--repo", "https://github.com/example/tools?token=x"),
            ("--commit", "abc1234"),
            ("--tag", "v1..v2"),
        ] {
            let mut args = submission_args(registry.path());
            let position = args.iter().position(|x| x == flag).unwrap();
            args[position + 1] = value.into();
            assert!(run(args, &project.root).is_err());
            assert!(!registry.path().join("extensions").exists());
        }
        let mut args = submission_args(registry.path());
        args.extend(["--category".into(), "prompts".into()]);
        assert!(run(args, &project.root).is_err());
        let mut args = submission_args(registry.path());
        args.extend(["--repo".into(), "https://github.com/another/tools".into()]);
        assert!(run(args, &project.root).is_err());
        fs::remove_file(project.root.join("DESCRIPTION.md")).unwrap();
        assert!(run(submission_args(registry.path()), &project.root).is_err());
        assert!(!registry.path().join("extensions").exists());
    }

    #[test]
    fn mcp_cli_scaffolds_doctors_and_packages_without_a_js_toolchain() {
        let temp = tempfile::tempdir().unwrap();
        run(
            [
                "init",
                "Remote tools",
                "--mcp-url",
                "https://TOOLS.example:443/mcp",
                "--id",
                "com.example.remote",
            ]
            .map(String::from),
            temp.path(),
        )
        .unwrap();
        let root = temp.path().join("remote-tools");
        for forbidden in ["manifest.json", "package.json", "grain.d.ts", "src", "dist"] {
            assert!(!root.join(forbidden).exists(), "unexpected {forbidden}");
        }
        assert!(root.join("DESCRIPTION.md").is_file());
        let descriptor = grain_extension_checks::read_mcp_descriptor(&root).unwrap();
        assert_eq!(
            descriptor.authentication,
            grain_sdk::mcp::McpAuthentication::OAuth {}
        );
        assert_eq!(
            descriptor.transport,
            grain_sdk::mcp::McpTransport::StreamableHttp {
                url: "https://tools.example/mcp".into()
            }
        );
        assert!(run(["doctor".into()], &root)
            .unwrap()
            .contains("0 findings"));
        let output = run(["pack".into()], &root).unwrap();
        let artifact = root.join("com.example.remote-0.1.0.mcp.json");
        assert!(output.contains(".mcp.json"));
        assert_eq!(
            serde_json::from_slice::<grain_sdk::mcp::McpDescriptor>(&fs::read(&artifact).unwrap())
                .unwrap(),
            descriptor
        );
        assert!(run(["doctor".into()], &root).is_ok());
        assert!(run(["pack".into()], &root).is_ok());
        assert!(run(["dev".into()], &root)
            .unwrap_err()
            .to_string()
            .contains("no Grain worker"));
        assert!(run(["submit".into()], &root)
            .unwrap_err()
            .to_string()
            .contains("--registry"));
    }

    #[test]
    fn mcp_refusals_preserve_existing_source_and_artifact() {
        let temp = tempfile::tempdir().unwrap();
        for (url, auth) in [
            ("http://tools.example", "oauth"),
            ("https://localhost/mcp", "none"),
            ("https://user:secret@tools.example", "oauth"),
            ("https://tools.example", "bearer"),
        ] {
            assert!(init_mcp_project(temp.path(), "Bad", None, url, auth).is_err());
            assert!(!temp.path().join("bad").exists());
        }
        let project = init_mcp_project(
            temp.path(),
            "Good",
            None,
            "https://tools.example/mcp",
            "none",
        )
        .unwrap();
        let root = &project.root;
        let source = fs::read(root.join("mcp.json")).unwrap();
        assert!(init_mcp_project(
            temp.path(),
            "Good",
            None,
            "https://other.example/mcp",
            "oauth"
        )
        .is_err());
        assert!(run(["pack", "--output", "mcp.json"].map(String::from), root).is_err());
        assert!(run(
            ["pack", "--output", "manifest.json"].map(String::from),
            root
        )
        .is_err());
        assert!(!root.join("manifest.json").exists());
        assert_eq!(fs::read(root.join("mcp.json")).unwrap(), source);
        let artifact = root.join("com.example.good-0.1.0.mcp.json");
        run(["pack".into()], root).unwrap();
        let original = fs::read(&artifact).unwrap();
        let mut bad: serde_json::Value = serde_json::from_slice(&source).unwrap();
        bad["headers"] = serde_json::json!({"Authorization":"secret"});
        write_json(&root.join("mcp.json"), &bad).unwrap();
        assert!(run(["doctor".into()], root)
            .unwrap_err()
            .to_string()
            .contains("E_MCP_DESCRIPTOR"));
        assert!(run(["pack".into()], root).is_err());
        assert_eq!(fs::read(&artifact).unwrap(), original);
        fs::write(root.join("mcp.json"), &source).unwrap();
        fs::write(root.join("manifest.json"), "{}").unwrap();
        assert!(run(["doctor".into()], root)
            .unwrap_err()
            .to_string()
            .contains("E_PROJECT_KIND"));
        assert!(run(["pack".into()], root).is_err());
        assert_eq!(fs::read(&artifact).unwrap(), original);
    }

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
        assert!(project.manifest.activation.is_empty());
        assert!(project.manifest.contributes.shortcuts.is_empty());
        assert_eq!(project.manifest.contributes.actions[0].id, "hello");
        validate_scaffold(&project).unwrap();

        let declarations = fs::read_to_string(result.root.join("grain.d.ts")).unwrap();
        for internal in [
            "DaemonEvent",
            "PillAction",
            "HostFrame",
            "TranscriptionComplete",
        ] {
            assert!(
                !declarations.contains(internal),
                "internal type leaked: {internal}"
            );
        }
        assert!(declarations.contains("interface GrainError extends Error"));
        assert!(declarations
            .split_once("declare global")
            .is_some_and(|(_, global)| global.contains("interface GrainError extends Error")));
        assert!(declarations.contains("E_CAPABILITY_DENIED"));
        assert!(declarations.contains("const grain: GrainApi"));
        for capability in KNOWN_CAPABILITIES
            .iter()
            .filter(|cap| grain_sdk::manifest::tool_permission_allowed(cap))
        {
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

    fn write_test_icon(dir: &Path) {
        image::RgbaImage::new(512, 512)
            .save_with_format(dir.join("icon.png"), image::ImageFormat::Png)
            .unwrap();
    }

    #[test]
    fn submit_writes_a_valid_submission_into_the_registry() {
        let temp = tempfile::tempdir().unwrap();
        let project =
            init_project(temp.path(), "Hello Ext", Some("com.example.hello-ext")).unwrap();
        // A submittable extension must carry its icon (§13.3); the scaffold
        // declares the path, the author supplies the file.
        write_test_icon(&project.root);
        let registry = tempfile::tempdir().unwrap();
        let output = run(
            [
                "submit".into(),
                "--registry".into(),
                registry.path().display().to_string(),
                "--repo".into(),
                "https://github.com/example/hello".into(),
                "--tag".into(),
                "v1.0.0".into(),
                "--commit".into(),
                "1234567890abcdef1234567890abcdef12345678".into(),
                "--category".into(),
                "tools".into(),
                "--contact".into(),
                "maintainer@example.com".into(),
            ],
            &project.root,
        )
        .unwrap();
        assert!(output.contains("pull request"));
        let sub = registry
            .path()
            .join("extensions/com.example.hello-ext/submission.toml");
        let raw = fs::read_to_string(&sub).unwrap();
        assert!(raw.contains("id = \"com.example.hello-ext\""));
        assert!(raw.contains("commit = \"1234567890abcdef1234567890abcdef12345678\""));
        let parsed =
            grain_extension_checks::read_submission(sub.parent().unwrap(), "com.example.hello-ext")
                .unwrap();
        assert_eq!(
            parsed.artifact_kind,
            grain_sdk::distribution::ArtifactKind::Native
        );
        assert!(registry
            .path()
            .join("extensions/com.example.hello-ext/DESCRIPTION.md")
            .exists());
    }

    #[test]
    fn doctor_flags_a_fresh_scaffold_missing_its_icon() {
        // The scaffold declares "icon.png" but cannot invent the art, so a fresh
        // project is not yet submittable — doctor says exactly why (§13.3).
        // Everything else about the scaffold is clean.
        let temp = tempfile::tempdir().unwrap();
        let project = init_project(temp.path(), "Doctor Test", None).unwrap();
        let error = run(["doctor".into()], &project.root)
            .unwrap_err()
            .to_string();
        assert!(error.contains("E_ICON"), "{error}");
        assert!(error.contains("icon"), "{error}");
    }

    #[test]
    fn doctor_accepts_a_scaffold_once_its_icon_is_added() {
        let temp = tempfile::tempdir().unwrap();
        let project = init_project(temp.path(), "Doctor Test", None).unwrap();
        write_test_icon(&project.root);
        let output = run(["doctor".into()], &project.root).unwrap();
        assert!(output.starts_with("doctor: 0 findings"), "{output}");
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
    fn artifact_replacement_keeps_only_the_complete_new_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("tool.grainpack");
        fs::write(&output, b"old").unwrap();

        replace_artifact(&output, b"complete-new-pack").unwrap();

        assert_eq!(fs::read(&output).unwrap(), b"complete-new-pack");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
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
