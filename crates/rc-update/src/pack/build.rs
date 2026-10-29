//! `rc-pack update manifest build` and `bump`: the signed manifest of one release.
//!
//! The archives are read from `--assets` by the names the packages carry -
//! `rcommander-<v>-windows-x64.zip`, `rcommander-<v>-linux-<x64|arm64>.tar.gz`,
//! `rcommander-<v>-macos-<aarch64|x86_64>.zip` - each hashed and sized here; anything else in
//! the folder is not an update and is left out. The manifest is signed, read back under the
//! signing key (the round trip) and written to `--out`.
//!
//! Adapted from rDownloader's `crates/rd-pack/src/update_manifest.rs` at commit 26c8592a6: the
//! archives hashed here instead of read from `SHA256SUMS`, the channel named, `bump` added.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use chrono::{Duration, SubsecRound, Utc};
use clap::Args;
use ed25519_dalek::{SigningKey, pkcs8::DecodePrivateKey};
use rc_update::{
    Artifact, Channel, UpdateManifest,
    manifest::{self, DEFAULT_VALIDITY_DAYS, MAX_VALIDITY_DAYS, kind},
    sign::{self, EMBEDDED_KEYS, EmbeddedKey, TrustStore, UPDATE_KEY_ID},
};

use crate::KeyArgs;

/// The environment variable the release workflow hands the update key in (RC-100-32).
const KEY_ENV: &str = "RCOMMANDER_UPDATE_SIGNING_KEY";

#[derive(Args)]
pub(crate) struct BuildArgs {
    /// `stable` or `beta`; must agree with the version (a pre-release is beta).
    #[arg(long)]
    channel: String,
    /// The release version, `X.Y.Z` or `X.Y.Z-beta.N` (a leading `v` is dropped).
    #[arg(long)]
    version: String,
    /// The folder holding the release's archives.
    #[arg(long)]
    assets: PathBuf,
    /// The `https://` folder the archives are downloaded from, ending in `/`
    /// (`https://github.com/degoya/rCommander/releases/download/vX.Y.Z/`).
    #[arg(long)]
    base_url: String,
    #[command(flatten)]
    key: KeyArgs,
    /// The manifest file to write (`rcommander-update-<channel>.json`).
    #[arg(long)]
    out: PathBuf,
    /// `RELEASE-NOTES.md`; the version's section becomes the notes.
    #[arg(long)]
    notes: Option<PathBuf>,
    /// Whether the release changes the database schema; left out, the manifest does not say and
    /// every installation treats it as a change.
    #[arg(long, value_name = "true|false")]
    schema_change: Option<bool>,
    /// The sequence; defaults to the signing time in Unix seconds, which only grows.
    #[arg(long)]
    sequence: Option<u64>,
    /// Days until the manifest expires.
    #[arg(long, default_value_t = DEFAULT_VALIDITY_DAYS)]
    valid_days: i64,
}

#[derive(Args)]
pub(crate) struct BumpArgs {
    /// The signed manifest to sign again.
    file: PathBuf,
    #[command(flatten)]
    key: KeyArgs,
    /// Where the new manifest goes; the file itself when left out.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Days until the new manifest expires.
    #[arg(long, default_value_t = DEFAULT_VALIDITY_DAYS)]
    valid_days: i64,
}

/// Reads a PKCS#8 PEM private key.
pub(crate) fn read_key_file(path: &Path) -> Result<SigningKey> {
    let pem = std::fs::read_to_string(path)
        .with_context(|| format!("read signing key {}", path.display()))?;
    parse_key(&pem)
}

fn parse_key(pem: &str) -> Result<SigningKey> {
    SigningKey::from_pkcs8_pem(pem.trim())
        .map_err(|_| anyhow!("the signing key is no Ed25519 PKCS#8 private key"))
}

fn read_key(args: &KeyArgs) -> Result<SigningKey> {
    match &args.key {
        Some(path) => read_key_file(path),
        None => parse_key(
            &std::env::var(KEY_ENV).with_context(|| format!("pass --key or set {KEY_ENV}"))?,
        ),
    }
}

pub(crate) fn build(args: &BuildArgs) -> Result<()> {
    build_under(args, EMBEDDED_KEYS)
}

/// [`build`] against the update roots `roots` - the compiled-in ones but in its tests.
pub(crate) fn build_under(args: &BuildArgs, roots: &[EmbeddedKey]) -> Result<()> {
    ensure!(
        (1..=MAX_VALIDITY_DAYS).contains(&args.valid_days),
        "--valid-days must be between 1 and {MAX_VALIDITY_DAYS}"
    );
    let version = rc_update::parse_version(&args.version)
        .with_context(|| format!("{} is not a SemVer version", args.version))?;
    let channel = Channel::parse(&args.channel)
        .with_context(|| format!("unknown channel {}", args.channel))?;
    ensure!(
        Channel::of_version(&version.to_string()) == channel,
        "{version} does not belong on the {channel} channel"
    );
    ensure!(
        args.base_url.starts_with("https://") && args.base_url.ends_with('/'),
        "--base-url must be an https:// folder ending in /"
    );
    let key = read_key(&args.key)?;
    check_against_root(&key, roots)?;
    let artifacts = artifacts(&args.assets, &version.to_string(), &args.base_url)?;
    let notes = match &args.notes {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read {}", path.display()))?;
            crate::notes::user_notes(&text, &version.to_string())?.unwrap_or_else(|| {
                eprintln!(
                    "warning: {} has no section for {version}; the manifest carries no notes",
                    path.display()
                );
                String::new()
            })
        }
        None => String::new(),
    };
    let issued_at = Utc::now().trunc_subsecs(0);
    let sequence = match args.sequence {
        Some(sequence) => sequence,
        None => u64::try_from(issued_at.timestamp()).context("the clock is before 1970")?,
    };
    let update = UpdateManifest {
        schema_version: manifest::UPDATE_MANIFEST_SCHEMA_VERSION,
        sequence,
        issued_at,
        not_after: issued_at + Duration::days(args.valid_days),
        channel,
        version: version.to_string(),
        released_at: issued_at,
        notes,
        artifacts,
        schema_change: args.schema_change,
    };
    write_signed(&key, &update, &args.out)
}

pub(crate) fn bump(args: &BumpArgs) -> Result<()> {
    ensure!(
        (1..=MAX_VALIDITY_DAYS).contains(&args.valid_days),
        "--valid-days must be between 1 and {MAX_VALIDITY_DAYS}"
    );
    let key = read_key(&args.key)?;
    check_against_root(&key, EMBEDDED_KEYS)?;
    let bytes =
        std::fs::read(&args.file).with_context(|| format!("read {}", args.file.display()))?;
    let trust = signing_trust(&key)?;
    // Read under the signing key: a manifest only its own publisher re-signs. The channel comes
    // from the payload; any expiry is fine, the point is a new one.
    let channel = [Channel::Stable, Channel::Beta]
        .into_iter()
        .find(|channel| channel.file_name() == file_name(&args.file))
        .unwrap_or_else(|| channel_of(&bytes));
    let old =
        manifest::verify_with(&bytes, &trust, channel, None, Utc::now()).or_else(|error| {
            match error {
                rc_update::UpdateError::Stale(_) => lenient(&bytes, &trust, channel),
                other => Err(anyhow!(
                    "{} does not verify ({}): {other}",
                    args.file.display(),
                    other.code()
                )),
            }
        })?;
    let issued_at = Utc::now().trunc_subsecs(0);
    let sequence = u64::try_from(issued_at.timestamp()).context("the clock is before 1970")?;
    ensure!(
        sequence > old.sequence,
        "the new sequence {sequence} is not above the old {}",
        old.sequence
    );
    let update = UpdateManifest {
        sequence,
        issued_at,
        not_after: issued_at + Duration::days(args.valid_days),
        ..old
    };
    write_signed(&key, &update, args.out.as_ref().unwrap_or(&args.file))
}

/// An expired manifest's payload, still under its signature: what `bump` renews.
fn lenient(bytes: &[u8], trust: &TrustStore, channel: Channel) -> Result<UpdateManifest> {
    let document = sign::SignedDocument::parse(bytes)?;
    let manifest: UpdateManifest = document
        .verify(manifest::UPDATE_MANIFEST_DOMAIN, trust)
        .map_err(|error| anyhow!("the manifest does not verify: {error}"))?;
    ensure!(
        manifest.channel == channel,
        "the manifest is not for {channel}"
    );
    Ok(manifest)
}

fn channel_of(bytes: &[u8]) -> Channel {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|document| {
            document["payload"]["channel"]
                .as_str()
                .and_then(Channel::parse)
        })
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn signing_trust(key: &SigningKey) -> Result<TrustStore> {
    let trust = TrustStore::new();
    trust.trust(UPDATE_KEY_ID.to_owned(), key.verifying_key())?;
    Ok(trust)
}

/// Signs, reads back under the signing key and writes `update` to `out`.
fn write_signed(key: &SigningKey, update: &UpdateManifest, out: &Path) -> Result<()> {
    let signed = manifest::sign(UPDATE_KEY_ID, key, update)?;
    manifest::verify_with(
        &signed,
        &signing_trust(key)?,
        update.channel,
        None,
        update.issued_at,
    )
    .map_err(|error| {
        anyhow!(
            "the signed manifest does not verify ({}): {error}",
            error.code()
        )
    })?;
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(out, &signed).with_context(|| format!("write {}", out.display()))?;
    println!(
        "signed {} {} with {} archives as sequence {} into {} (valid until {}, schema change: {})",
        update.channel,
        update.version,
        update.artifacts.len(),
        update.sequence,
        out.display(),
        update.not_after.to_rfc3339(),
        update
            .schema_change
            .map_or("not said", |change| if change { "yes" } else { "no" })
    );
    Ok(())
}

/// Refuses a key that is not the compiled-in root, so another role's key given by mistake is
/// caught here and not by every installation's check. While the root is the placeholder, a
/// warning: the manifest will verify nowhere until the public key is pasted in.
fn check_against_root(key: &SigningKey, table: &[EmbeddedKey]) -> Result<()> {
    let roots = sign::keys_at(table, Utc::now());
    match roots.iter().find(|root| root.key_id == UPDATE_KEY_ID) {
        Some(root) => ensure!(
            sign::decode_public_key(root.public_key)? == key.verifying_key(),
            "the key given is not the update root {UPDATE_KEY_ID} this build embeds"
        ),
        None => eprintln!(
            "warning: this build embeds no update root yet; paste the public key into \
             crates/rc-update/src/sign/roots.rs, or no installation verifies the manifest"
        ),
    }
    Ok(())
}

/// Every archive of `version` in `folder`, hashed and sized, sorted.
pub(crate) fn artifacts(folder: &Path, version: &str, base_url: &str) -> Result<Vec<Artifact>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder).with_context(|| format!("list {}", folder.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((platform, arch, archive_kind)) = classify(&name, version) else {
            continue;
        };
        let bytes = std::fs::read(entry.path()).with_context(|| format!("read {name}"))?;
        found.push(Artifact {
            platform: platform.to_owned(),
            arch: arch.to_owned(),
            kind: archive_kind.to_owned(),
            url: format!("{base_url}{name}"),
            sha256: sign::hex_sha256(&bytes),
            size: bytes.len() as u64,
        });
    }
    if found.is_empty() {
        bail!("{} holds no archive of {version}", folder.display());
    }
    found.sort_by(|left, right| {
        (&left.platform, &left.arch, &left.kind).cmp(&(&right.platform, &right.arch, &right.kind))
    });
    Ok(found)
}

/// `(platform, arch, kind)` of a package archive of `version`; `None` for anything else.
pub(crate) fn classify(
    name: &str,
    version: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    let rest = name.strip_prefix(&format!("rcommander-{version}-"))?;
    let (stem, archive_kind) = if let Some(stem) = rest.strip_suffix(".tar.gz") {
        (stem, kind::TAR_GZ)
    } else {
        (rest.strip_suffix(".zip")?, kind::ZIP)
    };
    let (platform, arch) = stem.split_once('-')?;
    let platform = match (platform, archive_kind) {
        ("windows", kind::ZIP) => "windows",
        ("macos", kind::ZIP) => "macos",
        ("linux", kind::TAR_GZ) => "linux",
        _ => return None,
    };
    let arch = match arch {
        "x64" | "x86_64" => "x86_64",
        "arm64" | "aarch64" => "aarch64",
        _ => return None,
    };
    Some((platform, arch, archive_kind))
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
