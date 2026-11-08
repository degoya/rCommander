//! A connection's local port forwards (RC-110-03, `connection_forwards`, migration 0028): a
//! listener on this computer whose connections an SSH session carries to a host and port as the
//! server sees it (`ssh -L`).
//!
//! Only SSH connections have them, at most [`MAX_FORWARDS`] each. The bind address is an IP
//! address, never a name to look up; whether one other than loopback may be bound is the
//! engine's to judge when the forward starts (`ssh.forwards_lan`), not the store's. No column
//! holds a secret.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{
    Db,
    connections::{ConnectionError, ConnectionId, ConnectionKind, TreeResult, fetch, validate},
};

/// The most port forwards one connection keeps: a bound, not a setting.
pub const MAX_FORWARDS: usize = 32;

/// The bind address a forward gets when none is given: loopback, which no firewall asks about.
pub const LOOPBACK: &str = "127.0.0.1";

/// The only kind for now: a local forward. The column is text for remote and dynamic ones later.
const LOCAL: &str = "local";

/// One stored port forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forward {
    pub id: i64,
    pub connection_id: ConnectionId,
    /// An IP address of this computer: `127.0.0.1` or `::1` unless `ssh.forwards_lan` is on.
    pub bind_address: String,
    pub bind_port: u16,
    /// The host the server connects to, as the server resolves it.
    pub target_host: String,
    pub target_port: u16,
    /// Started when the connection's session comes up.
    pub auto_start: bool,
    pub name: Option<String>,
    /// Its place among the connection's forwards, smallest first.
    pub sort_order: i64,
}

impl Forward {
    /// `127.0.0.1:5432 -> db:5432`, as the status bar and the form show it.
    #[must_use]
    pub fn label(&self) -> String {
        let bind = match self.bind_address.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => format!("[{}]", self.bind_address),
            _ => self.bind_address.clone(),
        };
        format!(
            "{bind}:{} \u{2192} {}:{}",
            self.bind_port, self.target_host, self.target_port
        )
    }
}

/// A forward to store. Wider than `u16` so that 0 or 70000 reach the check and its
/// `forward_invalid` instead of failing to parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardDraft {
    #[serde(default = "loopback")]
    pub bind_address: String,
    pub bind_port: i64,
    pub target_host: String,
    pub target_port: i64,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub name: Option<String>,
}

fn loopback() -> String {
    LOOPBACK.to_owned()
}

/// A draft's columns, checked.
struct Checked {
    bind_address: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
    auto_start: bool,
    name: Option<String>,
}

impl ForwardDraft {
    /// The columns of this draft, or why they are refused.
    fn checked(&self) -> TreeResult<Checked> {
        let invalid = |reason| ConnectionError::ForwardInvalid { reason };
        let bind_address = self.bind_address.trim();
        let bind_address = if bind_address.is_empty() {
            LOOPBACK.to_owned()
        } else {
            bind_address
                .parse::<IpAddr>()
                .map_err(|_| invalid("the bind address is an IP address of this computer"))?
                .to_string()
        };
        let port = |raw: i64, reason| {
            u16::try_from(raw)
                .ok()
                .filter(|port| *port != 0)
                .ok_or(invalid(reason))
        };
        let target_host = validate::host(&self.target_host)
            .map_err(|_| invalid("the target is a host name or address without spaces"))?;
        let name = validate::optional(self.name.as_deref());
        if name
            .as_ref()
            .is_some_and(|name| name.chars().count() > 255 || name.chars().any(char::is_control))
        {
            return Err(invalid("the name is at most 255 characters of text"));
        }
        Ok(Checked {
            bind_address,
            bind_port: port(self.bind_port, "the local port is 1 to 65535")?,
            target_host,
            target_port: port(self.target_port, "the target port is 1 to 65535")?,
            auto_start: self.auto_start,
            name,
        })
    }
}

/// `SELECT <every forward column> FROM connection_forwards <tail>`, as one static string.
macro_rules! select_forwards {
    ($tail:literal) => {
        concat!(
            "SELECT id, connection_id, bind_address, bind_port, target_host, target_port, \
             auto_start, name, sort_order FROM connection_forwards ",
            $tail
        )
    };
}

impl Db {
    /// The port forwards of connection `id`, in their order.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn forwards(&self, id: ConnectionId) -> TreeResult<Vec<Forward>> {
        let rows = sqlx::query(select_forwards!(
            "WHERE connection_id = ? AND kind = 'local' ORDER BY sort_order, id"
        ))
        .bind(id.0)
        .fetch_all(self.readers())
        .await?;
        Ok(rows
            .iter()
            .map(from_row)
            .collect::<sqlx::Result<Vec<_>>>()?)
    }

    /// The port forward `id`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ForwardNotFound`], or the read fails.
    pub async fn forward(&self, id: i64) -> TreeResult<Forward> {
        let mut reader = self.readers().acquire().await?;
        fetch_forward(&mut reader, id).await
    }

    /// Adds a port forward to the SSH connection `id`, after its last one.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ForwardInvalid`] (also for a connection that is not SSH),
    /// [`ConnectionError::ForwardsTooMany`], the connection missing, or the write fails.
    pub async fn create_forward(
        &self,
        id: ConnectionId,
        draft: &ForwardDraft,
    ) -> TreeResult<Forward> {
        let checked = draft.checked()?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        if fetch(&mut tx, id).await?.kind != ConnectionKind::Ssh {
            return Err(ConnectionError::ForwardInvalid {
                reason: "only an SSH connection forwards ports",
            });
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM connection_forwards WHERE connection_id = ?")
                .bind(id.0)
                .fetch_one(&mut *tx)
                .await?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_FORWARDS {
            return Err(ConnectionError::ForwardsTooMany { max: MAX_FORWARDS });
        }
        let done = sqlx::query(
            "INSERT INTO connection_forwards (connection_id, kind, bind_address, bind_port, \
             target_host, target_port, auto_start, name, sort_order) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, (SELECT ifnull(max(sort_order) + 1, 0) \
             FROM connection_forwards WHERE connection_id = ?))",
        )
        .bind(id.0)
        .bind(LOCAL)
        .bind(&checked.bind_address)
        .bind(i64::from(checked.bind_port))
        .bind(&checked.target_host)
        .bind(i64::from(checked.target_port))
        .bind(checked.auto_start)
        .bind(&checked.name)
        .bind(id.0)
        .execute(&mut *tx)
        .await?;
        let forward = fetch_forward(&mut tx, done.last_insert_rowid()).await?;
        tx.commit().await?;
        Ok(forward)
    }

    /// Replaces the fields of the port forward `id` with `draft`'s; it keeps its connection and
    /// place.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ForwardInvalid`], [`ConnectionError::ForwardNotFound`], or the write
    /// fails.
    pub async fn update_forward(&self, id: i64, draft: &ForwardDraft) -> TreeResult<Forward> {
        let checked = draft.checked()?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let done = sqlx::query(
            "UPDATE connection_forwards SET bind_address = ?, bind_port = ?, target_host = ?, \
             target_port = ?, auto_start = ?, name = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(&checked.bind_address)
        .bind(i64::from(checked.bind_port))
        .bind(&checked.target_host)
        .bind(i64::from(checked.target_port))
        .bind(checked.auto_start)
        .bind(&checked.name)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() == 0 {
            return Err(ConnectionError::ForwardNotFound { id });
        }
        let forward = fetch_forward(&mut tx, id).await?;
        tx.commit().await?;
        Ok(forward)
    }

    /// Deletes the port forward `id`; the deleted one.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ForwardNotFound`], or the write fails.
    pub async fn delete_forward(&self, id: i64) -> TreeResult<Forward> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let forward = fetch_forward(&mut tx, id).await?;
        sqlx::query("DELETE FROM connection_forwards WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(forward)
    }
}

/// The forward `id`, read inside a transaction or on a reader.
async fn fetch_forward(connection: &mut SqliteConnection, id: i64) -> TreeResult<Forward> {
    let row = sqlx::query(select_forwards!("WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ConnectionError::ForwardNotFound { id })?;
    Ok(from_row(&row)?)
}

fn from_row(row: &SqliteRow) -> sqlx::Result<Forward> {
    let port = |column: &str| -> sqlx::Result<u16> {
        let raw: i64 = row.try_get(column)?;
        u16::try_from(raw).map_err(|error| sqlx::Error::ColumnDecode {
            index: column.to_owned(),
            source: Box::new(error),
        })
    };
    Ok(Forward {
        id: row.try_get("id")?,
        connection_id: ConnectionId(row.try_get("connection_id")?),
        bind_address: row.try_get("bind_address")?,
        bind_port: port("bind_port")?,
        target_host: row.try_get("target_host")?,
        target_port: port("target_port")?,
        auto_start: row.try_get("auto_start")?,
        name: row.try_get("name")?,
        sort_order: row.try_get("sort_order")?,
    })
}

#[cfg(test)]
#[path = "forwards_tests.rs"]
mod tests;
