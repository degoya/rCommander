//! `rc-pack update manifest verify <file> [--assets <dir>]`: the manifest against this build's
//! update root - or against `--public-key`, for a pipeline that checks before the root is pasted
//! in - and, with `--assets`, every listed archive present there against its size and SHA-256.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::Args;
use rc_update::{
    Channel, UpdateManifest, manifest,
    sign::{self, TrustStore, UPDATE_KEY_ID},
};

#[derive(Args)]
pub(crate) struct VerifyArgs {
    /// The signed manifest; its channel is read from its file name unless `--channel` names it.
    file: PathBuf,
    /// `stable` or `beta`.
    #[arg(long)]
    channel: Option<String>,
    /// The folder holding the listed archives; each one present is checked.
    #[arg(long)]
    assets: Option<PathBuf>,
    /// A base64 public key (or a file holding one) to verify against instead of the root.
    #[arg(long)]
    public_key: Option<String>,
}

pub(crate) fn run(args: &VerifyArgs) -> Result<()> {
    let channel = channel(args)?;
    let bytes = std::fs::read(&args.file)
        .with_context(|| format!("read manifest {}", args.file.display()))?;
    let now = Utc::now();
    let verified = match &args.public_key {
        Some(given) => {
            let text = match std::fs::read_to_string(given) {
                Ok(text) => text,
                Err(_) => given.clone(),
            };
            let trust = TrustStore::new();
            trust.trust(UPDATE_KEY_ID.to_owned(), sign::decode_public_key(&text)?)?;
            manifest::verify_with(&bytes, &trust, channel, None, now)
        }
        None => manifest::verify(&bytes, channel, None, now),
    }
    .map_err(|error| {
        anyhow!(
            "{} does not verify ({}): {error}",
            args.file.display(),
            error.code()
        )
    })?;
    println!(
        "{} verifies: {} {} with {} archives, sequence {}, valid until {}",
        args.file.display(),
        channel,
        verified.version,
        verified.artifacts.len(),
        verified.sequence,
        verified.not_after.to_rfc3339()
    );
    if let Some(directory) = &args.assets {
        check_assets(&verified, directory)?;
    }
    Ok(())
}

fn channel(args: &VerifyArgs) -> Result<Channel> {
    if let Some(name) = &args.channel {
        return Channel::parse(name).with_context(|| format!("unknown channel {name}"));
    }
    let name = args
        .file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    [Channel::Stable, Channel::Beta]
        .into_iter()
        .find(|channel| channel.file_name() == name)
        .with_context(|| format!("{name} names no channel; pass --channel"))
}

/// Every listed archive present in `directory` has its size and SHA-256.
pub(crate) fn check_assets(verified: &UpdateManifest, directory: &Path) -> Result<()> {
    for artifact in &verified.artifacts {
        let name = artifact.url.rsplit('/').next().unwrap_or(&artifact.url);
        let path = directory.join(name);
        let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        if bytes.len() as u64 != artifact.size || sign::hex_sha256(&bytes) != artifact.sha256 {
            bail!("{} does not match its manifest entry", path.display());
        }
    }
    println!(
        "  every listed archive in {} matches its size and SHA-256",
        directory.display()
    );
    Ok(())
}
