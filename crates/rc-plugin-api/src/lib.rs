//! rCommander's plugin contract (RC-100-08): the WIT package `rcommander:plugin@0.3.0` and the
//! native side of each world, so a plugin's logic is tested without a WebAssembly toolchain.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-api` at commit 26c8592a6 (`src/lib.rs`,
//! `wit/rdownloader.wit`): one WIT world per plugin kind, a native trait per world. Stage 1 has
//! one world, `viewer-plugin` ([`viewer`]); contract 0.3.0 adds `secrets-plugin` ([`secrets`],
//! RC-100-33); the others arrive with their stages (`docs/plugins.md`, "Staged implementation").
//!
//! The crate builds for `wasm32-unknown-unknown` as well: a plugin's guest half translates the
//! generated WIT bindings into the trait here and nothing else.

#![warn(unreachable_pub)]

pub mod secrets;
pub mod viewer;

/// The in-memory host a plugin's native tests run against; never part of a component or the host.
#[cfg(all(any(test, feature = "test-support"), not(target_arch = "wasm32")))]
pub mod test_support;

pub use rc_plugin_types::{Failure, FailureKind, MessageParams};

/// The WIT package, exactly as the host's bindings and every plugin template are generated from.
pub const WIT: &str = include_str!("../wit/rcommander.wit");

/// The version of `rcommander:plugin` this contract is; a manifest's `api_version`.
pub const API_VERSION: &str = "0.3.0";

/// The package name every interface of the contract is imported and exported under.
#[cfg(test)]
pub const PACKAGE: &str = "rcommander:plugin";

/// A source handle: valid for the call or session it was handed out in, never a path.
pub type Source = u32;

/// A sink handle: the target the host opened for one call.
pub type Sink = u32;

/// What a source is, as `source.info` answers: never its folder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceInfo {
    pub name: String,
    pub size: u64,
    /// RFC 3339, when the backend knows it.
    pub modified: Option<String>,
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod contract_tests;
