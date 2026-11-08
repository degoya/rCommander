//! Why a `usercmd.*` or `start_menu.*` command, or a start as administrator, was refused
//! (RC-100-86), each with a stable code - a button bar's refusal keeps its own (RC-100-87); the
//! window explains it under `error.<code>`. No message repeats a parameter or a file name: a
//! parameter may hold a password.

use rc_db::UserCommandStoreError;

use super::{StartItemId, model::MAX_NAME_TAIL};
use crate::EngineError;

/// A refusal of an own command, a Start menu row or a start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UserCommandError {
    /// A name that is no `em_` and 1 to 40 of `a-z`, `0-9`, `_`.
    #[error("{name:?} is no own command's name: em_ and 1 to {MAX_NAME_TAIL} of a-z, 0-9, _")]
    Name { name: String },
    /// Another own command has this name.
    #[error("another own command is named {name:?}")]
    NameTaken { name: String },
    /// No own command has this name.
    #[error("there is no own command {name:?}")]
    Unknown { name: String },
    /// A command that does not fit: empty, a control character, a field too long, too many rows.
    #[error("the command does not fit: {reason}")]
    Invalid { reason: String },
    /// A row of an internal command no keymap row and no Total Commander name rCommander knows.
    #[error("{row:?} is no row rCommander runs")]
    UnknownRow { row: String },
    /// The own commands call each other deeper than 8 levels: one calls itself.
    #[error("{name:?} calls itself or own commands deeper than 8 levels")]
    Recursion { name: String },
    /// A value put into a batch file's parameters holds a character cmd would read as syntax.
    #[error(
        "a name holds one of % ^ & | < > \" and the program is a batch file; nothing was started"
    )]
    UnsafeArgument,
    /// A row only the window runs, called over MCP.
    #[error("the row {row:?} is the window's; it does not run over MCP")]
    WindowOnly { row: String },
    /// A name the system's ANSI code page cannot hold, for `%L` or `%F`.
    #[error("a name does not fit the system's code page; use %UL or %UF")]
    ListEncoding,
    /// A placeholder of a panel that does not show a local folder (an archive, a server).
    #[error("the {panel} panel shows no local folder for its placeholders")]
    LocalOnly { panel: &'static str },
    /// The program is not on the search path.
    #[error("the program was not found on the search path")]
    ProgramNotFound,
    /// The program did not start.
    #[error("the program did not start: {source}")]
    Failed {
        #[source]
        source: std::io::Error,
    },
    /// The own commands or the Start menu hold their most rows.
    #[error("at most {max} are kept")]
    TooMany { max: usize },
    /// No row of the Start menu has this id.
    #[error("there is no Start menu row {id}")]
    StartUnknown { id: StartItemId },
    /// A Start menu row that does not fit: no title, no submenu as parent, too deep, a cycle.
    #[error("the Start menu row does not fit: {reason}")]
    StartInvalid { reason: String },
    /// `*` outside Windows: nothing started, not even without elevation.
    #[error("starting as administrator works on Windows only; nothing was started")]
    RunAsAdminUnsupported,
    /// Windows refused the start as administrator; its code.
    #[error("the start as administrator failed (Windows error {code})")]
    RunAsAdminFailed { code: u32 },
    /// No internal association of the file has this command, they are switched off, or the
    /// entry is a folder (RC-100-91).
    #[error("the file has no internal association with this command")]
    NoAssociation,
    /// A button bar or a button's run refused (RC-100-87); keeps the code of its
    /// [`crate::ButtonBarError`] (`ButtonBarError::ALL_CODES`).
    #[error(transparent)]
    ButtonBar(crate::ButtonBarError),
}

impl UserCommandError {
    /// Every code of these refusals.
    pub const ALL_CODES: &[&str] = &[
        "user_command_name",
        "user_command_name_taken",
        "user_command_unknown",
        "user_command_invalid",
        "user_command_unknown_row",
        "user_command_recursion",
        "user_command_unsafe_argument",
        "user_command_window_only",
        "user_command_list_encoding",
        "user_command_local_only",
        "user_command_not_found",
        "user_command_failed",
        "user_command_too_many",
        "start_menu_unknown",
        "start_menu_invalid",
        "run_as_admin_unsupported",
        "run_as_admin_failed",
        "association_unknown",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Name { .. } => "user_command_name",
            Self::NameTaken { .. } => "user_command_name_taken",
            Self::Unknown { .. } => "user_command_unknown",
            Self::Invalid { .. } => "user_command_invalid",
            Self::UnknownRow { .. } => "user_command_unknown_row",
            Self::Recursion { .. } => "user_command_recursion",
            Self::UnsafeArgument => "user_command_unsafe_argument",
            Self::WindowOnly { .. } => "user_command_window_only",
            Self::ListEncoding => "user_command_list_encoding",
            Self::LocalOnly { .. } => "user_command_local_only",
            Self::ProgramNotFound => "user_command_not_found",
            Self::Failed { .. } => "user_command_failed",
            Self::TooMany { .. } => "user_command_too_many",
            Self::StartUnknown { .. } => "start_menu_unknown",
            Self::StartInvalid { .. } => "start_menu_invalid",
            Self::RunAsAdminUnsupported => "run_as_admin_unsupported",
            Self::RunAsAdminFailed { .. } => "run_as_admin_failed",
            Self::NoAssociation => "association_unknown",
            Self::ButtonBar(error) => error.code(),
        }
    }
}

impl From<UserCommandError> for EngineError {
    fn from(error: UserCommandError) -> Self {
        Self::UserCommand(error)
    }
}

/// A failed write of the store as the engine's error: a refusal with its code, or the database's.
pub(crate) fn store_error(error: UserCommandStoreError) -> EngineError {
    let start = |reason: &str| -> EngineError {
        UserCommandError::StartInvalid {
            reason: reason.to_owned(),
        }
        .into()
    };
    match error {
        UserCommandStoreError::NotFound { id } => UserCommandError::StartUnknown { id }.into(),
        UserCommandStoreError::TooMany { max } => UserCommandError::TooMany { max }.into(),
        UserCommandStoreError::Field { field, max } => UserCommandError::Invalid {
            reason: format!("the {field} is longer than {max} characters"),
        }
        .into(),
        UserCommandStoreError::NotASubmenu { .. } => start("the parent is no submenu"),
        UserCommandStoreError::Cycle => start("a submenu cannot move into itself"),
        UserCommandStoreError::TooDeep { .. } => start("the Start menu nests at most 8 levels"),
        UserCommandStoreError::NotEmpty { .. } => start("the submenu still holds rows"),
        UserCommandStoreError::Db(error) => EngineError::Database(error),
    }
}
