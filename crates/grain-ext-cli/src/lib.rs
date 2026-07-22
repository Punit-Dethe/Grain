use std::fs;
use std::path::{Path, PathBuf};

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
            "Created {}\n\nScripted extensions use Node.js and esbuild for bundling.\n\nNext:\n  cd {}\n  npm install\n  grain-ext dev",
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
                    default_binding: None,
                }],
            },
        },
        entry: "src/main.ts".into(),
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
        "# {name}\n\nGrain extension id: `{id}`\n\n## Develop\n\n1. Install Node.js.\n2. Run `npm install` (this installs esbuild locally).\n3. Run `grain-ext dev` for build, load, watch, and hot reload.\n\nEdit `src/main.ts`; `grain.d.ts` is generated from the Grain SDK.\n"
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
        assert_eq!(project.entry, "src/main.ts");
        validate_scaffold(&project).unwrap();

        let declarations = fs::read_to_string(result.root.join("grain.d.ts")).unwrap();
        assert!(declarations.contains("export type DaemonEvent"));
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
}
