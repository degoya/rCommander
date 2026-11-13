//! The internal associations the window knows (RC-100-91): the rules and the two switches as the
//! engine reads them (`Engine::file_associations`), at the start and again whenever one of
//! `files.associations`, `files.associations_enabled` or `files.associations_all_matches`
//! changes, kept for the run as the own commands are (`user_commands/known.rs`), so the context
//! menu offers a file's commands without asking the engine while it opens.

use rc_engine::Associations;

use crate::app::known::Known;

static KNOWN: Known<Option<Associations>> = Known::new("associations.known", None);

/// The rules and switches from now on.
pub fn set(associations: Associations) {
    KNOWN.set(Some(associations));
}

/// The rules and switches; none and switched off before the first read.
pub fn get() -> Associations {
    KNOWN.get().unwrap_or_default()
}
