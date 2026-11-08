//! The internal associations the window knows (RC-100-91): the rules and the two switches as the
//! engine reads them (`Engine::file_associations`), at the start and again whenever one of
//! `files.associations`, `files.associations_enabled` or `files.associations_all_matches`
//! changes, kept for the run as the own commands are (`user_commands/known.rs`), so the context
//! menu offers a file's commands without asking the engine while it opens.

use std::sync::RwLock;

use rc_engine::Associations;

use crate::recover::RwLockOrRecover as _;

static KNOWN: RwLock<Option<Associations>> = RwLock::new(None);

/// The rules and switches from now on.
pub fn set(associations: Associations) {
    *KNOWN.write_or_recover("associations.known") = Some(associations);
}

/// The rules and switches; none and switched off before the first read.
pub fn get() -> Associations {
    KNOWN
        .read_or_recover("associations.known")
        .clone()
        .unwrap_or_default()
}
