//! Why a `button_bar.*` command was refused (RC-100-87), each with a stable code; the window
//! explains it under `error.<code>`. No message repeats a parameter, a tooltip or a file name: a
//! parameter may hold a password. The engine carries it as an own command's refusal
//! ([`crate::UserCommandError::ButtonBar`]): a button runs the one command model.

use rc_db::ButtonBarStoreError;

use super::{ButtonBarId, MAX_BUTTONS, MAX_SUB_BAR_DEPTH};
use crate::{EngineError, UserCommandError};

/// A refusal of a button bar or of a button's run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ButtonBarError {
    /// No bar has this id.
    #[error("there is no button bar {id}")]
    Unknown { id: ButtonBarId },
    /// A name that is empty, too long or holds a control character.
    #[error("a bar's name has 1 to 64 characters and no control character")]
    Name,
    /// Another bar has this name.
    #[error("another button bar has this name")]
    NameTaken,
    /// A spec that does not fit: a newer version, too many buttons, a field too long.
    #[error("the button bar does not fit: {reason}")]
    Invalid { reason: String },
    /// The bars hold their most already.
    #[error("at most {max} button bars are kept")]
    TooMany { max: usize },
    /// A sub-bar that opens one of the bars above it.
    #[error("a sub-bar would open a bar above it")]
    Cycle,
    /// Sub-bars nested deeper than [`MAX_SUB_BAR_DEPTH`].
    #[error("sub-bars nest at most {MAX_SUB_BAR_DEPTH} levels deep")]
    TooDeep,
    /// A bar another bar opens as its sub-bar is not deleted.
    #[error("another bar opens this bar as its sub-bar")]
    InUse,
    /// No button at this place of the bar: a separator, or past the last.
    #[error("the bar has no button {button}; it holds at most {MAX_BUTTONS}")]
    NoButton { button: usize },
    /// A button that opens a sub-bar runs nothing.
    #[error("the button opens a sub-bar; nothing runs")]
    SubBar,
    /// Files dropped on a button that runs no program.
    #[error("only a button that starts a program takes files")]
    NoFiles,
    /// A dropped file that is no absolute local path.
    #[error("a dropped file is no local path rCommander passes on")]
    FileInvalid,
    /// A `.bar` file that does not read: no file, too large, no `[Buttonbar]`.
    #[error("the .bar file does not read: {reason}")]
    Import { reason: String },
}

impl ButtonBarError {
    /// Every code of these refusals.
    pub const ALL_CODES: &[&str] = &[
        "button_bar_unknown",
        "button_bar_name",
        "button_bar_name_taken",
        "button_bar_invalid",
        "button_bar_too_many",
        "button_bar_cycle",
        "button_bar_too_deep",
        "button_bar_in_use",
        "button_bar_no_button",
        "button_bar_sub_bar",
        "button_bar_no_files",
        "button_bar_file_invalid",
        "button_bar_import",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown { .. } => "button_bar_unknown",
            Self::Name => "button_bar_name",
            Self::NameTaken => "button_bar_name_taken",
            Self::Invalid { .. } => "button_bar_invalid",
            Self::TooMany { .. } => "button_bar_too_many",
            Self::Cycle => "button_bar_cycle",
            Self::TooDeep => "button_bar_too_deep",
            Self::InUse => "button_bar_in_use",
            Self::NoButton { .. } => "button_bar_no_button",
            Self::SubBar => "button_bar_sub_bar",
            Self::NoFiles => "button_bar_no_files",
            Self::FileInvalid => "button_bar_file_invalid",
            Self::Import { .. } => "button_bar_import",
        }
    }
}

impl From<ButtonBarError> for EngineError {
    fn from(error: ButtonBarError) -> Self {
        Self::UserCommand(UserCommandError::ButtonBar(error))
    }
}

/// A failed write of the store as the engine's error: a refusal with its code, or the database's.
pub(crate) fn store_error(error: ButtonBarStoreError) -> EngineError {
    if error.is_unique_violation() {
        return ButtonBarError::NameTaken.into();
    }
    match error {
        ButtonBarStoreError::NotFound { id } => ButtonBarError::Unknown { id }.into(),
        ButtonBarStoreError::TooMany { max } => ButtonBarError::TooMany { max }.into(),
        ButtonBarStoreError::Name { .. } => ButtonBarError::Name.into(),
        ButtonBarStoreError::SpecTooLarge { max } => ButtonBarError::Invalid {
            reason: format!("the buttons take more than {max} bytes"),
        }
        .into(),
        ButtonBarStoreError::Db(error) => EngineError::Database(error),
    }
}
