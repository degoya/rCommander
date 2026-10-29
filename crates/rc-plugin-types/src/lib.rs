//! The vocabulary rCommander's plugins share with the host (RC-100-08).
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-types` at commit 26c8592a6
//! (`src/error.rs`, `src/lib.rs`). rDownloader's failure classes are a scheduler's (transient,
//! rate limited, offline); rCommander's are a file manager's and mirror the WIT `failure-kind` of
//! `rcommander:plugin@0.1.0` one to one. The record carries no English message, as the WIT one
//! does not: a code and its parameters are what the window translates.

#![warn(unreachable_pub)]

mod failure;

pub use failure::{Failure, FailureKind, MessageParams};
