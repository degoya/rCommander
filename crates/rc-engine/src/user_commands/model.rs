//! The one command model (RC-100-86) of the own commands, the Start menu and - later - the button
//! bar (RC-100-87) and the file associations (RC-100-91): a [`CommandRef`] is a program with its
//! parameters, folder, window and elevation, one or more internal rows run one after the other,
//! or another own command `em_<name>`. Everything here is checked as untrusted input: it comes
//! from the window, MCP, an import or the database.

use serde::{Deserialize, Serialize};

use super::{MAX_ROWS, UserCommandError};

/// The id of a stored own command.
pub type UserCommandId = i64;

/// The id of a row of the Start menu.
pub type StartItemId = i64;

/// What an own command's name starts with, as in Total Commander.
pub const OWN_PREFIX: &str = "em_";

/// The most characters after [`OWN_PREFIX`].
pub const MAX_NAME_TAIL: usize = 40;

/// The most characters of one field (command, parameters, folder, tooltip, title).
pub const MAX_FIELD: usize = rc_db::MAX_USER_COMMAND_FIELD;

/// What [`CommandRef::command`] holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    /// A program: its path, or a name found on the search path (never in the panel's folder).
    #[default]
    Program,
    /// Internal rows separated by commas, run one after the other: names of the keymap and the
    /// palette (`copy`, `view_file`) or Total Commander's `cm_*` names (`tc_names.rs`), and
    /// other own commands `em_*`.
    Internal,
    /// Another own command, by its `em_` name.
    Own,
}

impl CommandKind {
    /// The text the database keeps.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Internal => "internal",
            Self::Own => "own",
        }
    }

    /// The kind the database text names; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "program" => Some(Self::Program),
            "internal" => Some(Self::Internal),
            "own" => Some(Self::Own),
            _ => None,
        }
    }
}

/// How a program's window opens (Total Commander's `iconic`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Show {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

impl Show {
    /// The text the database keeps.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
        }
    }

    /// The window the database text names; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "normal" => Some(Self::Normal),
            "minimized" => Some(Self::Minimized),
            "maximized" => Some(Self::Maximized),
            _ => None,
        }
    }
}

/// What a command does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRef {
    #[serde(default)]
    pub kind: CommandKind,
    /// The program; the rows separated by commas; or the `em_` name. `%VAR%` in a program is
    /// the environment's (`%COMMANDER_PATH%` rCommander's folder).
    pub command: String,
    /// A program's parameters with Total Commander's placeholders (`%P%N`, `%S`, `%UL`, ...);
    /// `?` in front asks for them before the start.
    #[serde(default)]
    pub params: String,
    /// The folder a program starts in; empty for the panel's.
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub show: Show,
    /// The program starts as administrator (`*` in Total Commander); Windows only.
    #[serde(default)]
    pub elevated: bool,
}

impl CommandRef {
    /// The rows of an internal command, trimmed, empty ones left out.
    #[must_use]
    pub fn rows(&self) -> Vec<&str> {
        self.command
            .split(',')
            .map(str::trim)
            .filter(|row| !row.is_empty())
            .collect()
    }

    /// Whether the program asks for its parameters before it starts (`?` in front).
    #[must_use]
    pub fn asks(&self) -> bool {
        self.kind == CommandKind::Program && self.params.trim_start().starts_with('?')
    }

    /// The parameters without the `?` that asks for them.
    #[must_use]
    pub fn params_to_ask(&self) -> &str {
        let params = self.params.trim_start();
        params.strip_prefix('?').unwrap_or(params)
    }

    /// The command checked, trimmed where blanks mean nothing.
    ///
    /// # Errors
    ///
    /// [`UserCommandError::Invalid`]: an empty command, a control character, a field too long,
    /// more than [`MAX_ROWS`] rows, a row or `em_` name that cannot be one, or elevation for
    /// anything but a program.
    pub fn checked(&self) -> Result<Self, UserCommandError> {
        let invalid = |reason: &str| UserCommandError::Invalid {
            reason: reason.to_owned(),
        };
        for (field, text) in [
            ("command", &self.command),
            ("params", &self.params),
            ("folder", &self.folder),
        ] {
            check_text(field, text)?;
        }
        let command = self.command.trim().to_owned();
        if command.is_empty() {
            return Err(invalid("a command needs a program, rows or an own command"));
        }
        let mut checked = Self {
            command,
            ..self.clone()
        };
        match self.kind {
            CommandKind::Program => {}
            CommandKind::Internal => {
                let rows = checked.rows();
                if rows.len() > MAX_ROWS {
                    return Err(invalid("an internal command runs at most 16 rows"));
                }
                if let Some(row) = rows.iter().find(|row| !is_row_name(row)) {
                    return Err(invalid(&format!("{row:?} is no row name")));
                }
                checked.command = rows.join(",");
            }
            CommandKind::Own => {
                checked.command = clean_name(&checked.command)?;
            }
        }
        if checked.kind != CommandKind::Program {
            if checked.elevated {
                return Err(invalid("only a program starts as administrator"));
            }
            checked.params.clear();
            checked.folder.clear();
            checked.show = Show::Normal;
        }
        Ok(checked)
    }
}

/// One own command `em_<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCommand {
    pub id: UserCommandId,
    /// `em_<name>`, lower case.
    pub name: String,
    pub action: CommandRef,
    /// What the Start menu, the palette and a button show; the name when empty.
    #[serde(default)]
    pub tooltip: String,
    /// The symbol's file and index (RC-100-87 shows it), or a symbol of rCommander's collection
    /// as `rc-symbol:<name>` (RC-110-10, [`super::symbols`]); empty for none.
    #[serde(default)]
    pub icon_path: String,
    #[serde(default)]
    pub icon_index: i64,
}

/// An own command to save: a new one, or over a stored one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCommandDraft {
    /// `em_<name>`; upper case is made lower, as Total Commander compares them.
    pub name: String,
    pub action: CommandRef,
    #[serde(default)]
    pub tooltip: String,
    #[serde(default)]
    pub icon_path: String,
    #[serde(default)]
    pub icon_index: i64,
}

/// What a row of the Start menu is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartItemKind {
    /// An entry that runs its command.
    #[default]
    Item,
    /// A submenu: the rows under it.
    Submenu,
    /// A line between rows.
    Separator,
}

impl StartItemKind {
    /// The text the database keeps.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Submenu => "submenu",
            Self::Separator => "separator",
        }
    }

    /// The kind the database text names; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "item" => Some(Self::Item),
            "submenu" => Some(Self::Submenu),
            "separator" => Some(Self::Separator),
            _ => None,
        }
    }
}

/// One row of the Start menu, in menu order with its depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartItem {
    pub id: StartItemId,
    /// The submenu it is in; `None` at the top.
    #[serde(default)]
    pub parent: Option<StartItemId>,
    /// 0 at the top, one more per submenu above it.
    pub depth: usize,
    pub kind: StartItemKind,
    /// What the menu shows; empty for a separator.
    #[serde(default)]
    pub title: String,
    /// An entry's command; `None` for a submenu or a separator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<CommandRef>,
}

/// A row of the Start menu to save.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartItemDraft {
    pub kind: StartItemKind,
    #[serde(default)]
    pub title: String,
    /// An entry's command; required for an entry, ignored for the other kinds.
    #[serde(default)]
    pub action: Option<CommandRef>,
}

impl StartItemDraft {
    /// The row checked: an entry has a title and a command, a submenu a title, a separator
    /// neither.
    ///
    /// # Errors
    ///
    /// [`UserCommandError::StartInvalid`], or the command's own refusal.
    pub fn checked(&self) -> Result<Self, UserCommandError> {
        let invalid = |reason: &str| UserCommandError::StartInvalid {
            reason: reason.to_owned(),
        };
        check_text("title", &self.title)?;
        let title = self.title.trim().to_owned();
        match self.kind {
            StartItemKind::Separator => Ok(Self {
                kind: StartItemKind::Separator,
                title: String::new(),
                action: None,
            }),
            StartItemKind::Submenu => {
                if title.is_empty() {
                    return Err(invalid("a submenu needs a title"));
                }
                Ok(Self {
                    kind: StartItemKind::Submenu,
                    title,
                    action: None,
                })
            }
            StartItemKind::Item => {
                if title.is_empty() {
                    return Err(invalid("an entry needs a title"));
                }
                let action = self
                    .action
                    .as_ref()
                    .ok_or_else(|| invalid("an entry needs a command"))?
                    .checked()?;
                Ok(Self {
                    kind: StartItemKind::Item,
                    title,
                    action: Some(action),
                })
            }
        }
    }
}

/// Refuses a field with a control character or longer than [`MAX_FIELD`] characters.
fn check_text(field: &str, text: &str) -> Result<(), UserCommandError> {
    if text.chars().any(char::is_control) {
        return Err(UserCommandError::Invalid {
            reason: format!("the {field} holds a control character"),
        });
    }
    if text.chars().count() > MAX_FIELD {
        return Err(UserCommandError::Invalid {
            reason: format!("the {field} is longer than {MAX_FIELD} characters"),
        });
    }
    Ok(())
}

/// `name` as an own command's name: trimmed, lower case (Total Commander compares them without
/// regard to case), `em_` and 1 to [`MAX_NAME_TAIL`] of `a-z`, `0-9`, `_`.
///
/// # Errors
///
/// [`UserCommandError::Name`] for anything else.
pub fn clean_name(name: &str) -> Result<String, UserCommandError> {
    let name = name.trim().to_ascii_lowercase();
    if own_name(&name).is_none() {
        return Err(UserCommandError::Name { name });
    }
    Ok(name)
}

/// The part after `em_` when `name` is an own command's name exactly as kept (lower case);
/// `None` for anything else.
#[must_use]
pub fn own_name(name: &str) -> Option<&str> {
    let tail = name.strip_prefix(OWN_PREFIX)?;
    let plain = !tail.is_empty()
        && tail.len() <= MAX_NAME_TAIL
        && tail
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    plain.then_some(tail)
}

/// Whether `row` can name a row of an internal command: a keymap name (`copy`, `view_file`), a
/// Total Commander name `cm_*` in any case, or an own command `em_*` in any case.
#[must_use]
pub fn is_row_name(row: &str) -> bool {
    let lower = row.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("cm_") {
        return !rest.is_empty()
            && lower.len() <= 64
            && rest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    }
    if lower.starts_with(OWN_PREFIX) {
        return own_name(&lower).is_some();
    }
    !row.is_empty()
        && row.len() <= 64
        && row
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.".contains(&byte))
}
