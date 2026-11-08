//! The stored own commands (RC-100-86): `usercmd.list`, `.save` and `.delete` over `rc_db`'s
//! `user_commands`. What goes in is checked here as at any boundary ([`UserCommandDraft`] through
//! [`clean_name`] and [`CommandRef::checked`]); what comes back is read as untrusted too: a row
//! whose kind or window does not read is left out with a warning. Every change announces
//! [`EngineEvent::UserCommandsChanged`].

use rc_db::{CommandFields, StoredUserCommand, UserCommandRecord};

use super::{
    CommandKind, CommandRef, Show, UserCommand, UserCommandDraft, UserCommandError, UserCommandId,
    UserCommandsOutcome, answer, clean_name, store_error,
};
use crate::{Engine, EngineError, EngineEvent, Outcome};

/// `action` as the database keeps it.
pub(super) fn fields_of(action: &CommandRef) -> CommandFields {
    CommandFields {
        kind: action.kind.as_str().to_owned(),
        command: action.command.clone(),
        params: action.params.clone(),
        folder: action.folder.clone(),
        show: action.show.as_str().to_owned(),
        elevated: action.elevated,
    }
}

/// The stored `fields` as a command; `None` when the kind or the window is none this build
/// knows, or the command does not pass the checks it passed on the way in.
pub(super) fn action_of(fields: &CommandFields) -> Option<CommandRef> {
    let action = CommandRef {
        kind: CommandKind::parse(&fields.kind)?,
        command: fields.command.clone(),
        params: fields.params.clone(),
        folder: fields.folder.clone(),
        show: Show::parse(&fields.show)?,
        elevated: fields.elevated,
    };
    action.checked().ok()
}

/// The row `stored` as an own command; `None` when it does not read.
fn command_of(stored: StoredUserCommand) -> Option<UserCommand> {
    let StoredUserCommand { id, record } = stored;
    let name = clean_name(&record.name).ok()?;
    let action = action_of(&record.fields)?;
    Some(UserCommand {
        id,
        name,
        action,
        tooltip: record.tooltip,
        icon_path: record.icon_path,
        icon_index: record.icon_index,
    })
}

impl Engine {
    /// Every own command, by name; a row that does not read is left out with a warning.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails.
    pub async fn user_commands(&self) -> Result<Vec<UserCommand>, EngineError> {
        let stored = self.db()?.user_commands().await.map_err(store_error)?;
        Ok(stored
            .into_iter()
            .filter_map(|row| {
                let id = row.id;
                let command = command_of(row);
                if command.is_none() {
                    tracing::warn!(id, "a stored own command does not read and is left out");
                }
                command
            })
            .collect())
    }

    /// The own command `name` (case does not count).
    ///
    /// # Errors
    ///
    /// [`UserCommandError::Name`], [`UserCommandError::Unknown`], or the read fails.
    pub(crate) async fn user_command(&self, name: &str) -> Result<UserCommand, EngineError> {
        let name = clean_name(name)?;
        let stored = self
            .db()?
            .user_command_named(&name)
            .await
            .map_err(store_error)?;
        Ok(stored
            .and_then(command_of)
            .ok_or(UserCommandError::Unknown { name })?)
    }

    /// `usercmd.list`.
    pub(crate) async fn list_user_commands(&self) -> Result<Outcome, EngineError> {
        let commands = self.user_commands().await?;
        Ok(answer(UserCommandsOutcome::Commands { commands }))
    }

    /// `usercmd.save`.
    pub(crate) async fn save_user_command(
        &self,
        id: Option<UserCommandId>,
        draft: UserCommandDraft,
    ) -> Result<Outcome, EngineError> {
        let id = self.store_user_command(id, &draft).await?;
        Ok(answer(UserCommandsOutcome::Saved { id }))
    }

    /// Stores `draft` as a new own command or over `id`, checked; the id it has. The import of
    /// Total Commander's commands stores through here too.
    pub(crate) async fn store_user_command(
        &self,
        id: Option<UserCommandId>,
        draft: &UserCommandDraft,
    ) -> Result<UserCommandId, EngineError> {
        let name = clean_name(&draft.name)?;
        let action = draft.action.checked()?;
        for (field, text) in [("tooltip", &draft.tooltip), ("icon", &draft.icon_path)] {
            if text.chars().any(char::is_control) {
                return Err(UserCommandError::Invalid {
                    reason: format!("the {field} holds a control character"),
                }
                .into());
            }
        }
        // A symbol of the collection must be one it has (RC-110-10).
        super::symbols::check(&draft.icon_path)
            .map_err(|reason| UserCommandError::Invalid { reason })?;
        let record = UserCommandRecord {
            name: name.clone(),
            fields: fields_of(&action),
            tooltip: draft.tooltip.trim().to_owned(),
            icon_path: draft.icon_path.trim().to_owned(),
            icon_index: draft.icon_index,
        };
        let stored =
            self.db()?
                .save_user_command(id, &record)
                .await
                .map_err(|error| match error {
                    rc_db::UserCommandStoreError::NotFound { .. } => {
                        UserCommandError::Unknown { name: name.clone() }.into()
                    }
                    error if error.is_unique_violation() => {
                        UserCommandError::NameTaken { name: name.clone() }.into()
                    }
                    error => store_error(error),
                })?;
        self.publish(EngineEvent::UserCommandsChanged);
        Ok(stored.id)
    }

    /// `usercmd.delete`.
    pub(crate) async fn delete_user_command(
        &self,
        id: UserCommandId,
    ) -> Result<Outcome, EngineError> {
        if !self
            .db()?
            .delete_user_command(id)
            .await
            .map_err(store_error)?
        {
            return Err(UserCommandError::Unknown {
                name: id.to_string(),
            }
            .into());
        }
        self.publish(EngineEvent::UserCommandsChanged);
        Ok(answer(UserCommandsOutcome::Deleted { removed: 1 }))
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
