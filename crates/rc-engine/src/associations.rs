//! Internal file associations (RC-100-91), as Total Commander's Files > "Internal associations
//! (Total Commander only)" has them: rules by file type, each a mask in Total Commander's syntax
//! (`*.txt *.log`, masks after `|` excluded), a description, a symbol, the command `open` - what
//! Enter and a double click run ahead of the system's association - and up to [`MAX_VERBS`]
//! named commands (verbs) for the context menu, each a [`CommandRef`] of RC-100-86. The first
//! rule whose mask matches counts; "Load extra commands from all matching types"
//! (`files.associations_all_matches`) adds the verbs of the others to the menu. An archive is
//! taken over only by a mask that names its extension (`*.zip`, never `*` or `*.*`), as Total
//! Commander 11.51 does; a folder is never associated.
//!
//! The rules are the protected setting `files.associations` (JSON text, checked by
//! [`check_rules`]), like `files.viewer`: which program runs is a person's choice, MCP only reads
//! it. A program needs a file parameter - `%1` (the file, `"%P%N"`), `%2` (every marked file,
//! `%P%S`) or one of RC-100-86's (`%N`, `%S`, `%L` ...) -, as Total Commander asks: "You need to
//! put %1 or "%1" (or %2) behind the program name"; nothing is appended.
//!
//! - `model.rs` - the rules, their check and the setting's text;
//! - `matching.rs` - which rule and which command a file gets, `%1` and `%2`;
//! - `run.rs` - Enter (`file.open`), `file.run_association`, `file.associations` and what the
//!   MCP gate reads.

mod matching;
mod model;
mod run;

use serde::{Deserialize, Serialize};

pub use matching::{
    OPEN_VERB, applies, command_for, commands_for, file_params, is_archive_name, names_extension,
    open_for,
};
pub use model::{
    AssociationVerb, FileAssociation, MAX_BYTES, MAX_RULES, MAX_TEXT, MAX_VERBS, RuleError,
    RuleErrorKind, check_rules, has_file_parameter, parse_rules, write_rules,
};

use crate::user_commands::CommandRef;

/// The rules and the two switches, as the window and the context menu read them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Associations {
    /// In the order they are tried.
    pub rules: Vec<FileAssociation>,
    /// `files.associations_enabled`: off, Enter and the menu use none of them.
    pub enabled: bool,
    /// `files.associations_all_matches`: the menu offers the verbs of every matching rule.
    pub all_matches: bool,
}

impl Associations {
    /// The menu's commands for the file `name`: none while switched off.
    #[must_use]
    pub fn commands_for(&self, name: &str) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        commands_for(&self.rules, name, is_archive_name(name), self.all_matches)
    }

    /// The command `verb` for the file `name`; none while switched off.
    #[must_use]
    pub fn command_for(&self, name: &str, verb: &str) -> Option<&CommandRef> {
        if !self.enabled {
            return None;
        }
        command_for(
            &self.rules,
            name,
            is_archive_name(name),
            self.all_matches,
            verb,
        )
    }
}

/// One command of each new variant, for the registry's tests.
#[cfg(test)]
pub(crate) fn one_of_each() -> Vec<crate::Command> {
    use crate::{Command, PanelId};
    vec![
        Command::AssociationsWindow {},
        Command::RunAssociation {
            panel: Some(PanelId::Left),
            path: Some("notes.log".into()),
            verb: "view".into(),
            params: None,
        },
    ]
}
