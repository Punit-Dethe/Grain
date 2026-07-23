//! `grain-registry` — maintainer tooling for the extension registry (Phase 5A).
//!
//! This binary is the *producing* side of the trust system; the app is the
//! *verifying* side. It never ships in Grain. Three jobs:
//!
//! - `keygen`  — generate a minisign keypair (root or publishing) and print the
//!               base64 public-key line to pin in the app.
//! - `sign`    — produce a detached `<file>.minisig` for a JSON document.
//! - `publine` — print the base64 public-key line from a `.pub` file.
//!
//! Signatures are non-prehashed Ed25519, the same shape Grain's updater already
//! trusts (C-10), and verify against `minisign-verify` in `grain-core`.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use minisign::{KeyPair, PublicKeyBox, SecretKeyBox};

#[derive(Parser)]
#[command(name = "grain-registry", about = "Grain extension registry maintainer tools")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a minisign keypair. Writes <name>.pub and <name>.key into --out,
    /// and prints the base64 public-key line to pin in the binary.
    Keygen {
        /// Directory to write the key files into (created if missing).
        #[arg(long)]
        out: PathBuf,
        /// Base file name, e.g. `root-a`, `root-b`, `publishing`.
        #[arg(long)]
        name: String,
    },
    /// Sign a JSON document, writing <in>.minisig beside it (or to --out).
    Sign {
        /// Secret key file (`.key`).
        #[arg(long)]
        key: PathBuf,
        /// The file to sign.
        #[arg(long, value_name = "FILE")]
        input: PathBuf,
        /// Signature output path (defaults to <input>.minisig).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print the base64 public-key line from a `.pub` file.
    Publine {
        #[arg(long)]
        pubkey: PathBuf,
    },
    /// Write and sign a `roots.json` naming the publishing key + base URLs.
    /// Signed with a ROOT key (the app pins the root public keys).
    Roots {
        /// Root secret key (`root-a.key`).
        #[arg(long)]
        root_key: PathBuf,
        /// The publishing public-key line (from `publine`).
        #[arg(long)]
        publishing_pub: String,
        /// One or more absolute base URLs the client fetches from.
        #[arg(long = "base-url")]
        base_urls: Vec<String>,
        /// Output directory (writes roots.json + roots.json.minisig).
        #[arg(long)]
        v1: PathBuf,
    },
    /// Publish a built `.grainpack` into a `v1/` tree: hash it, copy to
    /// blob/<sha256>.grainpack, upsert its index entry, bump + sign the index.
    Publish {
        /// Publishing secret key (`publishing.key`).
        #[arg(long)]
        key: PathBuf,
        /// The built artifact (JSON or ZIP `.grainpack`).
        #[arg(long)]
        pack: PathBuf,
        /// Trust rung to assign: verified | core | experimental | dev.
        #[arg(long, default_value = "verified")]
        trust: String,
        #[arg(long, default_value = "")]
        repo: String,
        #[arg(long, default_value = "")]
        commit: String,
        #[arg(long, default_value = "")]
        author: String,
        /// Days until the index `expires` (default 30).
        #[arg(long, default_value_t = 30)]
        expires_days: i64,
        /// The `v1/` output directory.
        #[arg(long)]
        v1: PathBuf,
    },
    /// Verify a `v1/` tree exactly as the app would: roots against the PINNED
    /// root keys, then index + revocations against the publishing key. Proves
    /// the producer and the client agree.
    Verify {
        #[arg(long)]
        v1: PathBuf,
    },
    /// Validate every `extensions/<id>/submission.toml` under a directory: the
    /// required fields, a reverse-DNS id matching the folder, and typosquat
    /// distance from existing ids. The check CI runs on every PR.
    CheckSubmission {
        #[arg(long)]
        dir: PathBuf,
    },
}

/// The submission manifest an author writes (source pointer, never an artifact).
#[derive(serde::Deserialize)]
struct Submission {
    id: String,
    source_repo: String,
    tag: String,
    commit: String,
    summary: String,
    #[serde(default)]
    categories: Vec<String>,
    license: String,
    contact: String,
}

fn check_submission(dir: PathBuf) -> Result<()> {
    let ext_dir = dir.join("extensions");
    let mut ids: Vec<String> = Vec::new();
    let mut problems = 0usize;

    let entries = fs::read_dir(&ext_dir)
        .with_context(|| format!("read {}", ext_dir.display()))?;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().to_string();
        let toml_path = entry.path().join("submission.toml");
        if !toml_path.exists() {
            println!("FAIL {folder}: missing submission.toml");
            problems += 1;
            continue;
        }
        let raw = fs::read_to_string(&toml_path)?;
        let s: Submission = match toml::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                println!("FAIL {folder}: unparseable submission.toml: {e}");
                problems += 1;
                continue;
            }
        };

        let fail = |msg: &str| {
            println!("FAIL {folder}: {msg}");
        };
        if s.id != folder {
            fail("id must match the folder name");
            problems += 1;
        }
        if !is_reverse_dns(&s.id) {
            fail("id must be reverse-DNS (e.g. com.example.thing)");
            problems += 1;
        }
        for (field, val) in [
            ("source_repo", &s.source_repo),
            ("tag", &s.tag),
            ("commit", &s.commit),
            ("summary", &s.summary),
            ("license", &s.license),
            ("contact", &s.contact),
        ] {
            if val.trim().is_empty() {
                fail(&format!("{field} is required"));
                problems += 1;
            }
        }
        if s.categories.is_empty() {
            fail("at least one category is required");
            problems += 1;
        }
        // Typosquat: reject a near-miss of an id already seen.
        for existing in &ids {
            if existing != &s.id && edit_distance(existing, &s.id) <= 1 {
                fail(&format!("id is a near-miss of existing '{existing}' (typosquat)"));
                problems += 1;
            }
        }
        ids.push(s.id.clone());
    }

    if problems == 0 {
        println!("OK — {} submission(s) valid", ids.len());
        Ok(())
    } else {
        anyhow::bail!("{problems} submission problem(s)")
    }
}

fn is_reverse_dns(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !p.starts_with('-')
        })
        // ':' would break `ext:<id>:<sid>` binding parsing.
        && !id.contains(':')
}

/// Levenshtein distance (small strings; typosquat check only).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { out, name } => keygen(out, name),
        Cmd::Sign { key, input, out } => sign(key, input, out),
        Cmd::Publine { pubkey } => publine(pubkey),
        Cmd::Roots {
            root_key,
            publishing_pub,
            base_urls,
            v1,
        } => roots(root_key, publishing_pub, base_urls, v1),
        Cmd::Publish {
            key,
            pack,
            trust,
            repo,
            commit,
            author,
            expires_days,
            v1,
        } => publish(key, pack, trust, repo, commit, author, expires_days, v1),
        Cmd::Verify { v1 } => verify(v1),
        Cmd::CheckSubmission { dir } => check_submission(dir),
    }
}

/// Run the client's own verification against a `v1/` tree, using the pinned root
/// keys compiled into `grain-core`. Fails loudly on any mismatch.
fn verify(v1: PathBuf) -> Result<()> {
    let roots_doc = fs::read(v1.join("roots.json")).context("read roots.json")?;
    let roots_sig = fs::read_to_string(v1.join("roots.json.minisig")).context("read roots sig")?;
    let roots = grain_core::trust::verify_roots(&roots_doc, &roots_sig)
        .map_err(|e| anyhow::anyhow!("roots verification failed (pinned key): {e}"))?;
    println!("roots.json OK — publishing key {}", &roots.publishing_key[..16.min(roots.publishing_key.len())]);

    let idx_doc = fs::read(v1.join("index.json")).context("read index.json")?;
    let idx_sig = fs::read_to_string(v1.join("index.json.minisig")).context("read index sig")?;
    let now = chrono::Utc::now().timestamp();
    let (index, status) =
        grain_core::trust::verify_index(&roots, &idx_doc, &idx_sig, None, now, false)
            .map_err(|e| anyhow::anyhow!("index verification failed: {e}"))?;
    println!(
        "index.json OK — {} entrie(s), status {:?}",
        index.entries.len(),
        status
    );
    for e in &index.entries {
        println!("  · {} {} [{}] {}", e.id, e.version, tier_dbg(&e.tier), e.sha256);
    }

    let rev_path = v1.join("revocations.json");
    if rev_path.exists() {
        let doc = fs::read(&rev_path)?;
        let sig = fs::read_to_string(v1.join("revocations.json.minisig"))?;
        let rev = grain_core::trust::verify_revocations(&roots, &doc, &sig)
            .map_err(|e| anyhow::anyhow!("revocations verification failed: {e}"))?;
        println!("revocations.json OK — {} entrie(s)", rev.entries.len());
    }
    println!("VERIFIED");
    Ok(())
}

fn tier_dbg(t: &grain_sdk::Tier) -> &'static str {
    match t {
        grain_sdk::Tier::Pack => "pack",
        grain_sdk::Tier::Scripted => "scripted",
        grain_sdk::Tier::Native => "native",
    }
}

/// Sign `bytes` into a detached `.minisig` string with a secret-key file.
fn sign_bytes(key: &std::path::Path, bytes: &[u8]) -> Result<String> {
    let sk_str = fs::read_to_string(key).with_context(|| format!("read {}", key.display()))?;
    let sk_box = SecretKeyBox::from_string(&sk_str).context("parse secret key")?;
    let sk = sk_box
        .into_secret_key(Some(String::new()))
        .context("decode secret key (unencrypted dev key)")?;
    let sig_box = minisign::sign(
        None,
        &sk,
        Cursor::new(bytes),
        Some("grain registry signed document"),
        Some("grain-registry"),
    )
    .context("sign")?;
    Ok(sig_box.into_string())
}

fn roots(
    root_key: PathBuf,
    publishing_pub: String,
    base_urls: Vec<String>,
    v1: PathBuf,
) -> Result<()> {
    fs::create_dir_all(&v1).ok();
    let roots = grain_sdk::Roots {
        spec: 1,
        version: 1,
        publishing_key: publishing_pub,
        base_urls,
        mirrors: Vec::new(),
        expires: None,
    };
    let json = format!("{}\n", serde_json::to_string_pretty(&roots)?);
    let doc = json.into_bytes();
    let sig = sign_bytes(&root_key, &doc)?;
    fs::write(v1.join("roots.json"), &doc)?;
    fs::write(v1.join("roots.json.minisig"), sig)?;
    println!("wrote {}/roots.json (+ .minisig)", v1.display());
    Ok(())
}

/// Read the manifest out of a `.grainpack` (JSON embeds it; ZIP carries a
/// `manifest.json` entry).
fn manifest_of(bytes: &[u8]) -> Result<grain_sdk::ExtensionManifest> {
    use grain_core::pack::{detect_shape, PackShape};
    match detect_shape(bytes) {
        PackShape::Json => {
            let pack: grain_sdk::GrainPack =
                serde_json::from_slice(bytes).context("parse JSON grainpack")?;
            Ok(pack.manifest)
        }
        PackShape::Zip => {
            let tmp = std::env::temp_dir().join(format!("grain-pub-{}", std::process::id()));
            let _ = fs::remove_dir_all(&tmp);
            fs::create_dir_all(&tmp)?;
            grain_core::pack::extract_zip(bytes, &tmp, Default::default())
                .map_err(|e| anyhow::anyhow!("extract: {e}"))?;
            let m = fs::read(tmp.join("manifest.json")).context("zip missing manifest.json")?;
            let manifest: grain_sdk::ExtensionManifest =
                serde_json::from_slice(&m).context("parse manifest.json")?;
            let _ = fs::remove_dir_all(&tmp);
            Ok(manifest)
        }
        PackShape::Unknown => anyhow::bail!("not a recognised .grainpack (first byte is not {{ or PK)"),
    }
}

#[allow(clippy::too_many_arguments)]
fn publish(
    key: PathBuf,
    pack: PathBuf,
    trust: String,
    repo: String,
    commit: String,
    author: String,
    expires_days: i64,
    v1: PathBuf,
) -> Result<()> {
    let bytes = fs::read(&pack).with_context(|| format!("read {}", pack.display()))?;
    let manifest = manifest_of(&bytes)?;
    let sha256 = grain_core::trust::sha256_hex(&bytes);

    let trust = match trust.as_str() {
        "core" => grain_sdk::Trust::Core,
        "verified" => grain_sdk::Trust::Verified,
        "experimental" => grain_sdk::Trust::Experimental,
        "dev" => grain_sdk::Trust::Dev,
        other => anyhow::bail!("unknown trust '{other}'"),
    };

    // Content-addressed blob.
    let blob_dir = v1.join("blob");
    fs::create_dir_all(&blob_dir)?;
    fs::write(blob_dir.join(format!("{sha256}.grainpack")), &bytes)?;

    // Load or start the index (we are the producer; no need to verify our own).
    let index_path = v1.join("index.json");
    let mut index: grain_sdk::Index = if index_path.exists() {
        serde_json::from_slice(&fs::read(&index_path)?).context("parse existing index.json")?
    } else {
        grain_sdk::Index {
            spec: 1,
            version: 0,
            expires: String::new(),
            entries: Vec::new(),
        }
    };

    let today = chrono::Utc::now();
    let entry = grain_sdk::IndexEntry {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        tier: manifest.tier.clone(),
        trust,
        capabilities: manifest.permissions.clone(),
        sha256,
        size: bytes.len() as u64,
        min_grain_api: manifest.grain_api.clone(),
        repo,
        source_commit: commit,
        author,
        reviewed_at: today.format("%Y-%m-%d").to_string(),
        reviewed_commit: String::new(),
        updated_at: today.format("%Y-%m-%d").to_string(),
        stars: 0,
    };

    // Upsert by (id, version).
    index
        .entries
        .retain(|e| !(e.id == entry.id && e.version == entry.version));
    index.entries.push(entry);
    index.version += 1;
    index.expires = (today + chrono::Duration::days(expires_days))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let json = format!("{}\n", serde_json::to_string_pretty(&index)?);
    let doc = json.into_bytes();
    let sig = sign_bytes(&key, &doc)?;
    fs::write(&index_path, &doc)?;
    fs::write(v1.join("index.json.minisig"), sig)?;

    println!(
        "published {} {} → {}/index.json (index version {})",
        manifest.id,
        manifest.version,
        v1.display(),
        index.version
    );
    Ok(())
}

fn keygen(out: PathBuf, name: String) -> Result<()> {
    fs::create_dir_all(&out).with_context(|| format!("create {}", out.display()))?;
    // Development keys are encrypted with an EMPTY passphrase (custody decision,
    // PHASE5A §Step 1): the box round-trips deterministically with no prompt,
    // and the final migration to real passphrases on removable media is a
    // re-pin + re-sign, not a redesign. `to_box`/`into_secret_key` must agree on
    // the passphrase, hence a generated-encrypted key rather than an
    // unencrypted one whose box the crate still wraps in scrypt.
    let KeyPair { pk, sk } =
        KeyPair::generate_encrypted_keypair(Some(String::new())).context("generate keypair")?;

    let pk_box = pk.to_box().context("encode public key")?;
    let sk_box = sk.to_box(Some(&format!("grain registry key: {name}"))).context("encode secret key")?;

    let pub_path = out.join(format!("{name}.pub"));
    let key_path = out.join(format!("{name}.key"));
    fs::write(&pub_path, pk_box.to_string()).with_context(|| format!("write {}", pub_path.display()))?;
    fs::write(&key_path, sk_box.to_string()).with_context(|| format!("write {}", key_path.display()))?;

    println!("wrote {}", pub_path.display());
    println!("wrote {}  (SECRET — never commit)", key_path.display());
    println!();
    println!("base64 public-key line to pin in the app:");
    println!("{}", pk.to_base64());
    Ok(())
}

fn sign(key: PathBuf, input: PathBuf, out: Option<PathBuf>) -> Result<()> {
    let sk_str = fs::read_to_string(&key).with_context(|| format!("read {}", key.display()))?;
    let sk_box = SecretKeyBox::from_string(&sk_str).context("parse secret key")?;
    // Pass an empty password rather than `None`: the minisign crate prompts on
    // stdin whenever the password is `None` (even for an unencrypted key), which
    // would hang non-interactive signing. Our development keys are unencrypted,
    // so an empty password is ignored by the (absent) KDF.
    let sk = sk_box
        .into_secret_key(Some(String::new()))
        .context("decode secret key (unencrypted dev key)")?;

    let data = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let sig_box = minisign::sign(
        None,
        &sk,
        Cursor::new(&data),
        Some("grain registry signed document"),
        Some("grain-registry"),
    )
    .context("sign")?;

    let sig_path = out.unwrap_or_else(|| {
        let mut p = input.clone().into_os_string();
        p.push(".minisig");
        PathBuf::from(p)
    });
    fs::write(&sig_path, sig_box.into_string()).with_context(|| format!("write {}", sig_path.display()))?;
    println!("wrote {}", sig_path.display());
    Ok(())
}

fn publine(pubkey: PathBuf) -> Result<()> {
    let s = fs::read_to_string(&pubkey).with_context(|| format!("read {}", pubkey.display()))?;
    let pk_box = PublicKeyBox::from_string(&s).context("parse public key")?;
    let pk = pk_box.into_public_key().context("decode public key")?;
    println!("{}", pk.to_base64());
    Ok(())
}
