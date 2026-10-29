//! `rcommander plugin ...`: what a plugin author runs (RC-100-12; `docs/plugins.md`, "SDK") -
//! `new` from the SDK template, `package` and sign, `verify` as installing would, `conformance`,
//! and `keygen` for a key pair.
//!
//! Copied and adapted from rDownloader's `crates/rd-pack/src/plugin.rs` at commit 26c8592a6: the
//! same subcommands and arguments, the library calls of `rc-plugin-host` (RC-100-09) behind
//! them. Adapted: no repository index (it comes with the store), the scaffolds compiled in
//! (`scaffold.rs`), the result lines in `locales/en.json`. Like `backup`, it attaches to the
//! caller's console on Windows.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use rc_plugin_host::{
    MAX_COMPONENT_BYTES, MAX_MANIFEST_BYTES, MAX_PACKAGE_BYTES, PluginVerifier, VerifyError,
};

use crate::{console, i18n::t};

mod scaffold;

/// The environment variable `package` reads the PEM private key from when `--key` is not given,
/// as the SDK's CI workflow signs.
pub const SIGNING_KEY_VARIABLE: &str = "RCOMMANDER_PLUGIN_SIGNING_KEY";

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Write a new plugin from the SDK template of its kind, with its own id and key pair.
    New(NewArgs),
    /// Build a signed .rcplug from a manifest, a component and its translations.
    Package(PackageArgs),
    /// Verify a package as installing it would, and say what it is.
    Verify(VerifyArgs),
    /// Check a package against everything this rCommander requires of it; exits 1 on a failure.
    Conformance(ConformanceArgs),
    /// Write a new Ed25519 key pair for signing plugins.
    Keygen(KeygenArgs),
}

#[derive(Args)]
pub struct NewArgs {
    /// The kind of plugin to write.
    #[arg(long = "type", value_name = "TYPE")]
    kind: scaffold::PluginKind,
    /// The folder to create; it must not exist.
    #[arg(long)]
    out: PathBuf,
    /// The plugin's name, and from it the crate's; defaults to the folder's name.
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
pub struct PackageArgs {
    #[arg(long)]
    manifest: PathBuf,
    /// Shared manifest fields (`plugins/metadata.toml`): each one the manifest leaves out is
    /// written into the packaged copy. A third party's manifest states them and needs none.
    #[arg(long)]
    defaults: Option<PathBuf>,
    /// The component `wasm-tools component new` made.
    #[arg(long)]
    component: PathBuf,
    /// A folder of `<lang>.json` translations, shipped and signed inside the package.
    #[arg(long)]
    locales: Option<PathBuf>,
    /// The PEM private key; without it, RCOMMANDER_PLUGIN_SIGNING_KEY holds the PEM text.
    #[arg(long)]
    key: Option<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    /// Write an unsigned package, which only development mode accepts.
    #[arg(long)]
    development: bool,
}

#[derive(Args)]
pub struct VerifyArgs {
    /// The `.rcplug` file.
    package: PathBuf,
    /// A key to trust beside the compiled-in ones, as KEY_ID=BASE64_PUBLIC_KEY; repeatable.
    #[arg(long = "trusted-key", value_name = "KEY_ID=KEY")]
    trusted_keys: Vec<String>,
    /// Accept an unsigned package, as development mode does.
    #[arg(long)]
    development_mode: bool,
}

#[derive(Args)]
pub struct ConformanceArgs {
    #[command(flatten)]
    package: VerifyArgs,
    /// A machine-readable report, for a CI job to act on.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct KeygenArgs {
    /// The file for the private key (PEM); it must not exist. Keep it out of version control.
    #[arg(long)]
    out: PathBuf,
}

/// Runs one subcommand and prints its result to the caller's console.
pub fn run(command: PluginCommand) -> ExitCode {
    console::attach_parent();
    match execute(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rcommander plugin: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn execute(command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::New(args) => new(&args),
        PluginCommand::Package(args) => package(&args),
        PluginCommand::Verify(args) => verify(&args),
        PluginCommand::Conformance(args) => conformance(&args),
        PluginCommand::Keygen(args) => keygen(&args),
    }
}

fn new(args: &NewArgs) -> Result<()> {
    let made = scaffold::scaffold(args.kind, &args.out, args.name.as_deref())?;
    println!(
        "{}",
        t!(
            "cli.plugin_scaffolded",
            name = made.name,
            path = args.out.display(),
            id = made.id,
            key = made.key_path.display()
        )
    );
    println!("{}", t!("cli.plugin_next_steps", slug = made.slug));
    Ok(())
}

fn package(args: &PackageArgs) -> Result<()> {
    let source = String::from_utf8(read_limited(&args.manifest, MAX_MANIFEST_BYTES as u64)?)
        .with_context(|| format!("{} is not UTF-8", args.manifest.display()))?;
    let component = read_limited(&args.component, MAX_COMPONENT_BYTES)?;
    let key = if args.development {
        None
    } else {
        let pem = match &args.key {
            Some(path) => std::fs::read_to_string(path)
                .with_context(|| format!("read the signing key {}", path.display()))?,
            None => std::env::var(SIGNING_KEY_VARIABLE).with_context(|| {
                format!("pass --key or set {SIGNING_KEY_VARIABLE} (or use --development)")
            })?,
        };
        Some(rc_plugin_host::load_signing_key_pem(&pem)?)
    };
    let locales = match &args.locales {
        Some(directory) => read_locales(directory)?,
        None => Vec::new(),
    };
    let defaults = args
        .defaults
        .as_deref()
        .map(rc_plugin_host::ManifestDefaults::read)
        .transpose()?;
    // The public half comes from the key itself; its name is not in a PEM, so `key_id` is the
    // manifest's or the shared file's.
    let public_key = key.as_ref().map(rc_plugin_host::public_key_base64);
    let manifest =
        rc_plugin_host::complete_manifest(&source, defaults.as_ref(), public_key.as_deref())
            .with_context(|| format!("complete the manifest {}", args.manifest.display()))?;
    let archive =
        rc_plugin_host::package_plugin(manifest.as_bytes(), &component, &locales, key.as_ref())?;
    if let Some(parent) = args.output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&args.output, &archive)
        .with_context(|| format!("write {}", args.output.display()))?;
    let signing = if key.is_some() {
        t!("cli.plugin_signed").to_owned()
    } else {
        t!("cli.plugin_unsigned").to_owned()
    };
    println!(
        "{}",
        t!(
            "cli.plugin_packaged",
            path = args.output.display(),
            size = archive.len(),
            signing = signing
        )
    );
    Ok(())
}

fn verify(args: &VerifyArgs) -> Result<()> {
    let verifier = verifier(args)?;
    let package = verifier
        .verify_file(&args.package)
        .map_err(describe_verify_error)?;
    let manifest = &package.manifest;
    let signing = if package.signature.is_some() {
        t!("cli.plugin_signed_by", key_id = manifest.key_id)
    } else {
        t!("cli.plugin_unsigned").to_owned()
    };
    println!(
        "{}",
        t!(
            "cli.plugin_verified",
            name = manifest.name,
            version = manifest.version,
            id = manifest.id,
            kind = manifest.plugin_type.as_str(),
            author = manifest.metadata.author,
            signing = signing
        )
    );
    Ok(())
}

fn conformance(args: &ConformanceArgs) -> Result<()> {
    let verifier = verifier(&args.package)?;
    let bytes = read_limited(&args.package.package, MAX_PACKAGE_BYTES)?;
    let runtime = tokio::runtime::Runtime::new().context("start the async runtime")?;
    let report = runtime.block_on(rc_plugin_host::check_package(&bytes, &verifier));
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report_text(&report));
    }
    if !report.passed {
        bail!("the package does not pass conformance");
    }
    Ok(())
}

/// The report as lines a person reads: the plugin, then one line per check and its detail.
fn report_text(report: &rc_plugin_host::ConformanceReport) -> String {
    let mut text = String::new();
    if let (Some(name), Some(version)) = (&report.name, &report.version) {
        let kind = report.plugin_type.as_deref().unwrap_or("?");
        text.push_str(&format!("{name} {version} ({kind})\n"));
    }
    for check in &report.checks {
        let mark = if check.passed { "ok  " } else { "FAIL" };
        text.push_str(&format!("  [{mark}] {} - {}\n", check.id, check.about));
        if let Some(detail) = &check.detail {
            text.push_str(&format!("         {detail}\n"));
        }
    }
    text
}

fn keygen(args: &KeygenArgs) -> Result<()> {
    let key = rc_plugin_host::generate_signing_key()?;
    scaffold::write_private(&args.out, key.private_pem.as_bytes())?;
    println!(
        "{}",
        t!(
            "cli.plugin_keygen_done",
            path = args.out.display(),
            public_key = key.public_base64,
            fingerprint = group_fingerprint(&rc_plugin_host::key_fingerprint(
                &key.signing_key.verifying_key()
            ))
        )
    );
    Ok(())
}

/// The compiled-in roots and every `--trusted-key`.
fn verifier(args: &VerifyArgs) -> Result<PluginVerifier> {
    let verifier = PluginVerifier::with_roots(args.development_mode)?;
    for value in &args.trusted_keys {
        let (key_id, encoded) = value
            .split_once('=')
            .context("a trusted key is written KEY_ID=BASE64_PUBLIC_KEY")?;
        verifier
            .trust_key_base64(key_id.to_owned(), encoded)
            .with_context(|| format!("the trusted key {key_id}"))?;
    }
    Ok(verifier)
}

/// An untrusted key as advice rather than a bare refusal.
fn describe_verify_error(error: VerifyError) -> anyhow::Error {
    match error {
        VerifyError::UntrustedKey {
            key_id,
            public_key,
            fingerprint,
            name,
            version,
        } => anyhow::anyhow!(
            "{name} {version} is signed by the untrusted key `{key_id}`\n  fingerprint: {}\n  \
             Pass --trusted-key {key_id}={public_key} only if this fingerprint is the one the \
             plugin's author published.",
            group_fingerprint(&fingerprint)
        ),
        VerifyError::Other(error) => error,
    }
}

/// A hex fingerprint in blocks of eight, to compare by eye.
fn group_fingerprint(fingerprint: &str) -> String {
    fingerprint
        .as_bytes()
        .chunks(8)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A file of at most `limit` bytes.
fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let size = std::fs::metadata(path)
        .with_context(|| format!("open {}", path.display()))?
        .len();
    if size > limit {
        bail!("{} is larger than {limit} bytes", path.display());
    }
    std::fs::read(path).with_context(|| format!("read {}", path.display()))
}

/// `<folder>/<lang>.json`, sorted by language; any other file in the folder is refused.
fn read_locales(directory: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut locales = Vec::new();
    let entries = std::fs::read_dir(directory)
        .with_context(|| format!("read the locales folder {}", directory.display()))?;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(language) = name
            .strip_suffix(".json")
            .filter(|tag| rc_plugin_host::valid_language(tag))
        else {
            bail!("{name} is not a `<lang>.json` locale file");
        };
        let bytes = read_limited(&entry.path(), rc_plugin_host::MAX_LOCALE_BYTES)?;
        locales.push((language.to_owned(), bytes));
    }
    locales.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(locales)
}

#[cfg(test)]
#[path = "plugin_cli_tests.rs"]
mod tests;
