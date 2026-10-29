//! `rc-pack`: the publishing half of the self-update (RC-100-04, wave 0.8-w30) - a small binary so
//! signing a release does not build the iced program (rDownloader's `rd-pack`, RD-150-20).
//!
//! * `rc-pack keygen --role <update|plugin> --output <dir>` - a fresh Ed25519 pair: the private
//!   PKCS#8 PEM (`0600`, never overwritten) and the base64 public key to paste into the roots.
//! * `rc-pack update manifest build ...` - the signed manifest of one release from its archives.
//! * `rc-pack update manifest verify <file> [--assets <dir>]` - against this build's root (or a
//!   given public key) and the archives' sizes and digests.
//! * `rc-pack update manifest bump <file> ...` - the same manifest signed again with a new sequence
//!   and expiry, so a long-lived stable release does not expire.
//!
//! The private key is read from `--key <file>` or `RCOMMANDER_UPDATE_SIGNING_KEY` (the PEM text)
//! and never printed.

mod build;
mod keygen;
mod notes;
mod verify;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rc-pack",
    version,
    about = "Sign and verify rCommander's update manifests"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate an Ed25519 key pair for a role.
    Keygen(keygen::KeygenArgs),
    /// The update manifest.
    Update {
        #[command(subcommand)]
        command: UpdateCommand,
    },
}

#[derive(Subcommand)]
enum UpdateCommand {
    /// Build, verify or re-sign a manifest.
    Manifest {
        #[command(subcommand)]
        command: ManifestCommand,
    },
}

#[derive(Subcommand)]
enum ManifestCommand {
    /// Build and sign the manifest of one release from its archives.
    Build(build::BuildArgs),
    /// Verify a signed manifest and, with `--assets`, the archives it lists.
    Verify(verify::VerifyArgs),
    /// Sign an existing manifest again with a new sequence and expiry.
    Bump(build::BumpArgs),
}

/// Where a private key comes from: `--key`, or the environment variable.
#[derive(Args)]
pub(crate) struct KeyArgs {
    /// PKCS#8 PEM private key, as `rc-pack keygen --role update` writes it; alternatively set
    /// RCOMMANDER_UPDATE_SIGNING_KEY to the PEM text.
    #[arg(long)]
    key: Option<PathBuf>,
}

fn main() -> ExitCode {
    let outcome = match Cli::parse().command {
        Command::Keygen(args) => keygen::run(&args),
        Command::Update {
            command: UpdateCommand::Manifest { command },
        } => match command {
            ManifestCommand::Build(args) => build::build(&args),
            ManifestCommand::Verify(args) => verify::run(&args),
            ManifestCommand::Bump(args) => build::bump(&args),
        },
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rc-pack: {error:#}");
            ExitCode::FAILURE
        }
    }
}
