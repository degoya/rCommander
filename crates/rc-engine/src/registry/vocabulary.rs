//! The fixed vocabulary of the registry: what a command may do, its row in [`super::COMMANDS`],
//! and where a cursor goes.

use serde::{Deserialize, Serialize};

/// What a command may do, which decides the MCP policy and whether a confirmation code is
/// needed - or, for [`CommandClass::GuiOnly`], that MCP cannot run it at all.
///
/// The classes have no order of their own: which of two is the stricter is
/// [`CommandClass::stricter`]'s, written out, not the order of the variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandClass {
    /// Changes no file: reading, listing, and moving the panels' view (path, cursor, selection).
    Read,
    /// Creates or changes files without losing data: mkdir, copy to a new name, write a new file.
    Write,
    /// Can lose data: delete, overwrite, move onto an existing name.
    Destructive,
    /// Runs something: a program, a remote command, input into a terminal.
    Exec,
    /// A human decision about trust or a secret: accepting or replacing an SSH host key,
    /// importing keys to trust, unlocking the vault, setting the master password. Only the GUI
    /// runs it; [`crate::Engine::dispatch_mcp`] refuses it with `gui_only`, whatever the policy.
    GuiOnly,
}

impl CommandClass {
    /// Every class, in order.
    pub const ALL: [Self; 5] = [
        Self::Read,
        Self::Write,
        Self::Destructive,
        Self::Exec,
        Self::GuiOnly,
    ];

    /// The class's name as the settings and the coverage page spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
            Self::Exec => "exec",
            Self::GuiOnly => "gui_only",
        }
    }

    /// The stricter of `self` and `other`: the one the gate holds a call to when both apply.
    /// `gui_only` is never run by MCP; a destructive call is asked every time, while an
    /// executing one may run on a grant for its target (`mcp.ask.exec_session_minutes`), so
    /// destructive is the stricter of those two; then write, then read.
    #[must_use]
    pub fn stricter(self, other: Self) -> Self {
        if other.strictness() > self.strictness() {
            other
        } else {
            self
        }
    }

    /// The rank [`CommandClass::stricter`] compares, higher is stricter.
    fn strictness(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Write => 1,
            Self::Exec => 2,
            Self::Destructive => 3,
            Self::GuiOnly => 4,
        }
    }
}

/// One row of the registry: a command's id, class and summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// The stable id, also the serde tag of the [`Command`] variant.
    pub id: &'static str,
    /// What the command may do.
    pub class: CommandClass,
    /// One line for the keymap editor, the menus and the coverage page.
    pub summary: &'static str,
}

/// A cursor movement from where the cursor stands, clamped to the panel's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStep {
    Up,
    Down,
    /// One page up: the rows the window last reported, less one.
    PageUp,
    PageDown,
    First,
    Last,
    /// One column back in the brief view; nothing in the full view.
    Left,
    /// One column on in the brief view; nothing in the full view.
    Right,
}

/// Where a cursor goes: an index into the panel's entries or an entry's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CursorTarget {
    Index(usize),
    Name(String),
}
