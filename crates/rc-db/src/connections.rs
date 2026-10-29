//! The connection tree: folders and the connections inside them (`connection_folders`,
//! `connections`, migration 0003).
//!
//! Every write validates first (`validate.rs`): a name is not blank and taken once per folder
//! (ignoring ASCII case), a host is one word, a port is 1 to 65535, a folder never moves into
//! its own subtree and folders nest at most [`MAX_DEPTH`] deep. Each write is one `BEGIN
//! IMMEDIATE` transaction on the serialized writer, so a check and its write see the same tree.
//!
//! An import (`import.rs`, RC-050-06) marks each connection with its source and key, so that
//! importing the same source again updates instead of duplicating; the export and import of
//! RC-080-19 read and write those marks through `origin.rs`.
//!
//! The manager's form is saved in one transaction (`save.rs`, RC-080-25): fields, secrets,
//! login script, shared credential and the drive bar's marks together, or nothing.
//!
//! A shared credential (`credentials.rs`, RC-050-12) sits in a folder beside the connections:
//! a user and secrets that any number of connections point at.
//!
//! No plaintext secret passes through here. The queries name their columns, so the encrypted
//! columns of migration 0006 are never read into these types; a connection carries only whether
//! each is set ([`crate::SecretFlags`]) and a stored key's public half ([`crate::StoredKey`]). The
//! secrets themselves are `secrets.rs`'s.

mod credentials;
mod endpoint;
mod error;
mod folders;
mod import;
mod model;
mod origin;
mod save;
mod store;
mod validate;

use sqlx::{Row as _, sqlite::SqliteRow};

use crate::{SecretFlags, StoredKey};

pub use credentials::{
    Credential, CredentialDraft, CredentialPatch, CredentialSave, CredentialTarget,
};
pub use endpoint::{EndpointRule, moves_endpoint};
pub use error::ConnectionError;
pub use import::{ImportCandidate, ImportResult, ImportStatus, ImportTarget};
pub use model::{
    AuthMethod, Connection, ConnectionDraft, ConnectionFolder, ConnectionId, ConnectionKind,
    ConnectionPatch, ConnectionTree, CredentialId, DefaultView, FolderDeletion, FolderId,
    FolderNode, FtpDataMode, FtpEncoding,
};
pub use origin::{ConnectionOrigin, INSTALLATION_KEY, check_draft};
pub use save::{ConnectionSave, SaveError, SaveTarget, SecretWrite};
pub(crate) use store::fetch;
pub use validate::{MAX_PATH_CHARS, MAX_TEXT_CHARS};

/// How deep folders may nest; the root's folders are level 1.
pub const MAX_DEPTH: usize = 32;

/// The result of every connection-tree call.
pub type TreeResult<T> = Result<T, ConnectionError>;

/// `SELECT <every connection column> FROM connections <tail>`, as one static string.
macro_rules! select_connections {
    ($tail:literal) => {
        concat!(
            "SELECT id, folder_id, name, kind, host, port, username, auth_method, key_path, \
             login_script_id, default_view, start_directory, notes, ftp_data_mode, ftp_encoding, \
             sort_order, created_at, updated_at, password_enc IS NOT NULL AS has_password, \
             key_enc IS NOT NULL AS has_key, passphrase_enc IS NOT NULL AS has_passphrase, \
             key_type, key_fingerprint, key_comment, key_public, credential_id FROM connections ",
            $tail
        )
    };
}

/// `SELECT <every folder column> FROM connection_folders <tail>`, as one static string.
macro_rules! select_folders {
    ($tail:literal) => {
        concat!(
            "SELECT id, parent_id, name, sort_order, created_at, updated_at \
             FROM connection_folders ",
            $tail
        )
    };
}

pub(crate) use select_connections;
pub(crate) use select_folders;

/// A TEXT column holding one of an enum's names.
fn text_column<T>(row: &SqliteRow, column: &str, parse: fn(&str) -> Option<T>) -> sqlx::Result<T> {
    let text: String = row.try_get(column)?;
    parse(&text).ok_or_else(|| sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        source: format!("unknown value {text:?}").into(),
    })
}

/// A nullable TEXT column holding one of an enum's names.
fn optional_text_column<T>(
    row: &SqliteRow,
    column: &str,
    parse: fn(&str) -> Option<T>,
) -> sqlx::Result<Option<T>> {
    let Some(text) = row.try_get::<Option<String>, _>(column)? else {
        return Ok(None);
    };
    parse(&text)
        .map(Some)
        .ok_or_else(|| sqlx::Error::ColumnDecode {
            index: column.to_owned(),
            source: format!("unknown value {text:?}").into(),
        })
}

/// A connection from a row of [`select_connections`].
pub(crate) fn connection_from_row(row: &SqliteRow) -> sqlx::Result<Connection> {
    let port: i64 = row.try_get("port")?;
    Ok(Connection {
        id: ConnectionId(row.try_get("id")?),
        folder_id: row.try_get::<Option<i64>, _>("folder_id")?.map(FolderId),
        name: row.try_get("name")?,
        kind: text_column(row, "kind", ConnectionKind::parse)?,
        host: row.try_get("host")?,
        port: u16::try_from(port).map_err(|error| sqlx::Error::ColumnDecode {
            index: "port".to_owned(),
            source: Box::new(error),
        })?,
        username: row.try_get("username")?,
        auth_method: text_column(row, "auth_method", AuthMethod::parse)?,
        key_path: row.try_get("key_path")?,
        login_script_id: row.try_get("login_script_id")?,
        default_view: text_column(row, "default_view", DefaultView::parse)?,
        start_directory: row.try_get("start_directory")?,
        notes: row.try_get("notes")?,
        ftp_data_mode: optional_text_column(row, "ftp_data_mode", FtpDataMode::parse)?,
        ftp_encoding: optional_text_column(row, "ftp_encoding", FtpEncoding::parse)?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        secrets: SecretFlags::from_row(row)?,
        key: StoredKey::from_row(row)?,
        credential_id: row
            .try_get::<Option<i64>, _>("credential_id")?
            .map(CredentialId),
    })
}

/// A folder from a row of [`select_folders`].
pub(crate) fn folder_from_row(row: &SqliteRow) -> sqlx::Result<ConnectionFolder> {
    Ok(ConnectionFolder {
        id: FolderId(row.try_get("id")?),
        parent_id: row.try_get::<Option<i64>, _>("parent_id")?.map(FolderId),
        name: row.try_get("name")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// A failed write as the tree reports it: a unique-index violation is the name taken (a second
/// process can still race the check inside the transaction), anything else the database's.
pub(crate) fn write_error(error: sqlx::Error, name: &str) -> ConnectionError {
    if let sqlx::Error::Database(database) = &error
        && database.is_unique_violation()
    {
        return ConnectionError::NameTaken {
            name: name.to_owned(),
        };
    }
    error.into()
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
