//! rCommander's self-update (RC-100-04 ... RC-100-07): is there a newer rCommander, can it be
//! trusted, and how does it replace the running one so that a stop anywhere leaves the old or the
//! new version whole.
//!
//! * [`sign`] - the length-prefixed digest, the signed-document format bound to a domain string,
//!   the replay rules and the compiled-in update root (`rcommander-update-v1`).
//! * [`manifest`] - the signed update manifest per channel and its verification: size, signature,
//!   schema, channel, freshness, content, in that order.
//! * [`check`] - fetching the manifests a channel reads from the GitHub releases (or a mirror) and
//!   verifying them against the replay floors the caller stores.
//! * [`fetch`] - https at every hop, bounded, the proxy; [`MemoryFetcher`] for tests.
//! * [`offer`] - which verified version is offered: never a downgrade, never a beta on stable.
//! * [`download`] - an artifact streamed to disk and kept only when size and SHA-256 match.
//! * [`state`] - what an installation remembers between starts (`update.state`).
//! * [`install_kind`] - whether this installation updates itself (the portable package only).
//! * [`install`] - the journalled switch, the updater's run, the roll-back and the recovery at
//!   start (RC-100-06).
//! * [`auto_install`] - when an offer installs by itself (RC-100-07).
//! * [`retention`] - what stays of the copies after a proven update (RC-100-07).
//!
//! No database, no engine, no iced: the updater runs this before the new version is known to
//! start. Copied and adapted from rDownloader (`crates/rd-update`, `crates/rd-sign`) at commit
//! 26c8592a6; every module names its source.

#![warn(unreachable_pub)]

pub mod auto_install;
pub mod check;
pub mod download;
mod error;
pub mod fetch;
pub mod install;
pub mod install_kind;
pub mod manifest;
pub mod offer;
pub mod retention;
pub mod sign;
pub mod state;

pub use check::{CheckReport, Floors, Sources, check};
pub use download::{Progress, check_space, download_verified, verified_file};
pub use error::{UPDATE_ERROR_CODES, UpdateError};
pub use fetch::{Fetcher, HttpFetcher, MemoryFetcher, ProxyChoice};
pub use install_kind::{InstallKind, Installation};
pub use manifest::{Artifact, Channel, UpdateManifest};
pub use offer::{Offer, Target, is_newer, newest_offer, parse_version};
pub use state::{STATE_KEY, UpdateState};
