//! Own commands (`user_commands`, migration 0027, RC-100-86): Total Commander's `em_<name>`
//! commands, each a program, a list of keymap rows or another own command, with its parameters,
//! folder, window and whether it starts as administrator.
//!
//! The engine checks what goes in (the name, the kind, the rows, the placeholders) and reads what
//! comes back as untrusted; this module keeps the text, holds the table to
//! [`MAX_USER_COMMANDS`] rows and every field to [`MAX_USER_COMMAND_FIELD`] characters. The
//! Start menu's tree is `start_menu.rs`; both tables are durable ([`crate::DURABLE_TABLES`]).

use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{Db, DbError};

/// The most own commands kept.
pub const MAX_USER_COMMANDS: usize = 512;

/// The most characters of one field (command, parameters, folder, tooltip, title, symbol).
pub const MAX_USER_COMMAND_FIELD: usize = 4096;

/// What a command does, as stored: the fields an own command and a Start menu entry share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandFields {
    /// `program`, `internal` or `own`.
    pub kind: String,
    /// The program, the keymap rows separated by commas, or the `em_` name.
    pub command: String,
    pub params: String,
    pub folder: String,
    /// `normal`, `minimized` or `maximized`.
    pub show: String,
    pub elevated: bool,
}

impl Default for CommandFields {
    fn default() -> Self {
        Self {
            kind: "program".to_owned(),
            command: String::new(),
            params: String::new(),
            folder: String::new(),
            show: "normal".to_owned(),
            elevated: false,
        }
    }
}

/// One own command to store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserCommandRecord {
    /// `em_<name>`, in lower case.
    pub name: String,
    pub fields: CommandFields,
    pub tooltip: String,
    /// The symbol's file and its index in it; empty for none.
    pub icon_path: String,
    pub icon_index: i64,
}

/// One own command as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredUserCommand {
    pub id: i64,
    pub record: UserCommandRecord,
}

/// Why a change of the own commands or the Start menu was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum UserCommandStoreError {
    /// No row has this id.
    #[error("there is no row {id}")]
    NotFound { id: i64 },
    /// The table holds its most rows already.
    #[error("at most {max} rows are kept")]
    TooMany { max: usize },
    /// A field is longer than [`MAX_USER_COMMAND_FIELD`] characters.
    #[error("the field {field} has more than {max} characters")]
    Field { field: &'static str, max: usize },
    /// The parent of a Start menu row is no submenu.
    #[error("row {id} is no submenu")]
    NotASubmenu { id: i64 },
    /// A submenu would move into itself or below.
    #[error("a submenu cannot move into itself or one of its submenus")]
    Cycle,
    /// The Start menu would nest deeper than [`crate::START_MENU_MAX_DEPTH`].
    #[error("the Start menu nests at most {max} levels deep")]
    TooDeep { max: usize },
    /// A submenu that still holds rows would become an entry or a separator.
    #[error("submenu {id} still holds rows")]
    NotEmpty { id: i64 },
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<sqlx::Error> for UserCommandStoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}

impl UserCommandStoreError {
    /// Whether a write failed on a unique constraint: a name another row has.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Db(error) if error.is_unique_violation())
    }
}

pub(crate) type StoreResult<T> = Result<T, UserCommandStoreError>;

/// Refuses a field longer than [`MAX_USER_COMMAND_FIELD`] characters.
pub(crate) fn check_field(field: &'static str, text: &str) -> StoreResult<()> {
    if text.chars().count() > MAX_USER_COMMAND_FIELD {
        return Err(UserCommandStoreError::Field {
            field,
            max: MAX_USER_COMMAND_FIELD,
        });
    }
    Ok(())
}

/// Refuses a command whose fields are too long.
pub(crate) fn check_fields(fields: &CommandFields) -> StoreResult<()> {
    check_field("command", &fields.command)?;
    check_field("params", &fields.params)?;
    check_field("folder", &fields.folder)
}

/// The command fields of a row read with the column names of `user_commands`, or with
/// `command_kind` for the kind (`start_menu_items`).
pub(crate) fn fields_from_row(row: &SqliteRow, kind_column: &str) -> sqlx::Result<CommandFields> {
    Ok(CommandFields {
        kind: row.try_get(kind_column)?,
        command: row.try_get("command")?,
        params: row.try_get("params")?,
        folder: row.try_get("folder")?,
        show: row.try_get("show")?,
        elevated: row.try_get("elevated")?,
    })
}

/// The columns of an own command; literal, so every query stays a literal.
macro_rules! columns {
    () => {
        "id, name, kind, command, params, folder, show, elevated, tooltip, icon_path, icon_index"
    };
}

fn command_from_row(row: &SqliteRow) -> sqlx::Result<StoredUserCommand> {
    Ok(StoredUserCommand {
        id: row.try_get("id")?,
        record: UserCommandRecord {
            name: row.try_get("name")?,
            fields: fields_from_row(row, "kind")?,
            tooltip: row.try_get("tooltip")?,
            icon_path: row.try_get("icon_path")?,
            icon_index: row.try_get("icon_index")?,
        },
    })
}

impl Db {
    /// Every own command, by name.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn user_commands(&self) -> StoreResult<Vec<StoredUserCommand>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM user_commands ORDER BY name, id"
        ))
        .fetch_all(self.readers())
        .await?;
        Ok(rows
            .iter()
            .map(command_from_row)
            .collect::<sqlx::Result<Vec<_>>>()?)
    }

    /// The own command named `name` (case does not count); `None` when there is none.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn user_command_named(&self, name: &str) -> StoreResult<Option<StoredUserCommand>> {
        let row = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM user_commands WHERE name = ? COLLATE NOCASE"
        ))
        .bind(name)
        .fetch_optional(self.readers())
        .await?;
        Ok(row.as_ref().map(command_from_row).transpose()?)
    }

    /// Stores `record` as a new own command (`id` `None`) or over the command `id`; the command
    /// as stored. The caller checks the name: one another command has fails the unique
    /// constraint.
    ///
    /// # Errors
    ///
    /// [`UserCommandStoreError::NotFound`], [`UserCommandStoreError::TooMany`] for a new one
    /// beyond [`MAX_USER_COMMANDS`], a field too long, a taken name, or the write fails; nothing
    /// is stored.
    pub async fn save_user_command(
        &self,
        id: Option<i64>,
        record: &UserCommandRecord,
    ) -> StoreResult<StoredUserCommand> {
        check_fields(&record.fields)?;
        check_field("tooltip", &record.tooltip)?;
        check_field("icon_path", &record.icon_path)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let id = match id {
            Some(id) => {
                fetch(&mut tx, id).await?;
                sqlx::query(
                    "UPDATE user_commands SET name = ?, kind = ?, command = ?, params = ?, \
                     folder = ?, show = ?, elevated = ?, tooltip = ?, icon_path = ?, \
                     icon_index = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                     WHERE id = ?",
                )
                .bind(&record.name)
                .bind(&record.fields.kind)
                .bind(&record.fields.command)
                .bind(&record.fields.params)
                .bind(&record.fields.folder)
                .bind(&record.fields.show)
                .bind(record.fields.elevated)
                .bind(&record.tooltip)
                .bind(&record.icon_path)
                .bind(record.icon_index)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                id
            }
            None => {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_commands")
                    .fetch_one(&mut *tx)
                    .await?;
                if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_USER_COMMANDS {
                    return Err(UserCommandStoreError::TooMany {
                        max: MAX_USER_COMMANDS,
                    });
                }
                sqlx::query(
                    "INSERT INTO user_commands (name, kind, command, params, folder, show, \
                     elevated, tooltip, icon_path, icon_index) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&record.name)
                .bind(&record.fields.kind)
                .bind(&record.fields.command)
                .bind(&record.fields.params)
                .bind(&record.fields.folder)
                .bind(&record.fields.show)
                .bind(record.fields.elevated)
                .bind(&record.tooltip)
                .bind(&record.icon_path)
                .bind(record.icon_index)
                .execute(&mut *tx)
                .await?
                .last_insert_rowid()
            }
        };
        let stored = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(stored)
    }

    /// Removes the own command `id`; whether there was one.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete_user_command(&self, id: i64) -> StoreResult<bool> {
        let mut writer = self.writer().await;
        let deleted = sqlx::query("DELETE FROM user_commands WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?
            .rows_affected();
        Ok(deleted > 0)
    }
}

async fn fetch(connection: &mut SqliteConnection, id: i64) -> StoreResult<StoredUserCommand> {
    let row = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM user_commands WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(UserCommandStoreError::NotFound { id })?;
    Ok(command_from_row(&row)?)
}

#[cfg(test)]
#[path = "user_commands_tests.rs"]
mod tests;
