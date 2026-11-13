//! What a panic leaves behind (RC-080-71), once for the workspace (RC-110-30): the helpers every
//! crate's `recover.rs` re-exports, so `crate::recover::...` names the same code everywhere.
//!
//! The code is in the module [`recover`], in a file of that name, because
//! `crates/rcommander/tests/repo_lints/poison.rs` allows `PoisonError::into_inner` in a
//! `recover.rs` and nowhere else.

mod recover;

pub use recover::{LockOrRecover, LogPanic, RwLockOrRecover, recovered};
