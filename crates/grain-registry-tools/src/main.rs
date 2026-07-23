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
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { out, name } => keygen(out, name),
        Cmd::Sign { key, input, out } => sign(key, input, out),
        Cmd::Publine { pubkey } => publine(pubkey),
    }
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
