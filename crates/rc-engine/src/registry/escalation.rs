//! The class a call really has: [`Command::effective_class`], from the command's typed
//! parameters. An overwriting copy loses data like a delete does, a login script with steps
//! types them on the next login - such a call is held to a stricter class than its command's
//! row, and [`ESCALATIONS`] names each case once: the MCP gate asks by it, the coverage page
//! (`scripts/mcp-coverage.sh`) shows it.
//!
//! Two cases depend on what is stored, not on the parameters alone - a retention limit below
//! the one in force, a saved queue list holding an overwriting job; `Engine::effective_class`
//! adds them (`dispatch/mcp.rs`).

use rc_ops::ConflictChoice;

use super::{Command, CommandClass};

/// One case of a command whose calls can be stricter than its row: the command's id, the class
/// such a call has and, in a few words, when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escalation {
    pub id: &'static str,
    pub class: CommandClass,
    pub when: &'static str,
}

/// Every escalation, one row per command, sorted by id. `scripts/mcp-coverage.sh` reads it from
/// the source, so a row keeps this exact shape - `Escalation { id: "...", class:
/// CommandClass::..., when: "..." },` - with no escaped quotes in `when`.
pub const ESCALATIONS: &[Escalation] = &[
    Escalation {
        id: "archive.extract",
        class: CommandClass::Destructive,
        when: "on_conflict overwrites",
    },
    Escalation {
        id: "archive.pack",
        class: CommandClass::Destructive,
        when: "it deletes its sources or replaces an archive",
    },
    Escalation {
        id: "connections.save",
        class: CommandClass::Exec,
        when: "it names a login script",
    },
    Escalation {
        id: "diagnostics.set_retention",
        class: CommandClass::Destructive,
        when: "a limit is lowered below the one in force",
    },
    Escalation {
        id: "login_scripts.create",
        class: CommandClass::Exec,
        when: "it brings steps",
    },
    Escalation {
        id: "login_scripts.update",
        class: CommandClass::Exec,
        when: "it sets steps",
    },
    Escalation {
        id: "ops.copy",
        class: CommandClass::Destructive,
        when: "on_conflict overwrites",
    },
    Escalation {
        id: "ops.move",
        class: CommandClass::Destructive,
        when: "on_conflict overwrites",
    },
    Escalation {
        id: "queue.load",
        class: CommandClass::Destructive,
        when: "the saved list holds a job that overwrites",
    },
    Escalation {
        id: "sync.apply",
        class: CommandClass::Destructive,
        when: "it may delete or overwrite",
    },
];

/// The escalation row of `id`.
#[must_use]
pub fn escalation(id: &str) -> Option<&'static Escalation> {
    ESCALATIONS.iter().find(|row| row.id == id)
}

/// Whether a conflict answered with `choice` replaces the existing target.
#[must_use]
pub(crate) fn overwrites(choice: Option<ConflictChoice>) -> bool {
    matches!(
        choice,
        Some(
            ConflictChoice::Overwrite
                | ConflictChoice::OverwriteOlder
                | ConflictChoice::OverwriteLarger
                | ConflictChoice::OverwriteSmaller
        )
    )
}

impl Command {
    /// The class this call has: its row's class, or - when its parameters make it so - the
    /// stricter class of its [`ESCALATIONS`] row ([`CommandClass::stricter`], never lower than
    /// the row). What is stored can make it stricter still (`Engine::effective_class`).
    #[must_use]
    pub fn effective_class(&self) -> CommandClass {
        let class = self.spec().class;
        if !self.escalates() {
            return class;
        }
        escalation(self.id()).map_or(class, |row| class.stricter(row.class))
    }

    /// Whether the parameters make this call one of its [`ESCALATIONS`] case.
    fn escalates(&self) -> bool {
        match self {
            Self::OpsCopy(request) | Self::OpsMove(request) => {
                overwrites(request.options.on_conflict)
            }
            Self::ArchiveExtract { options, .. } => overwrites(options.on_conflict),
            Self::ArchivePack { settings, .. } => settings.delete_sources || settings.overwrite,
            Self::SyncApply(request) => {
                request.run.deletes_confirmed || overwrites(request.run.on_conflict)
            }
            // Clearing it counts too, as it always has: the call names the script field.
            Self::SaveConnection(request) => request.login_script.is_some(),
            Self::CreateLoginScript { script } => !script.steps.is_empty(),
            Self::UpdateLoginScript { patch, .. } => patch.steps.is_some(),
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "escalation_tests.rs"]
mod tests;
