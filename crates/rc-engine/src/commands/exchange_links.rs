//! What an import of an export sets once every connection of it is written (RC-110-03): each
//! connection's jump host, named in the file by the mark of its own entry, and its local port
//! forwards.
//!
//! A jump host is found by its mark - this installation's own (its id) or one an earlier import
//! wrote - among the connections here after the writes. One that is neither in the file nor here
//! leaves the connection without a jump host and the row says so (`jump_missing`), in the
//! preview already. A forward already there (same port and target) is not added twice; the
//! forwards come in as the file has them, `auto_start` included: `config.import` is the person's
//! (`GuiOnly`).

use std::collections::HashSet;

use rc_db::connections::ConnectionOrigin;

use crate::{
    ConnectionId, ConnectionPatch, Engine, EngineError, ForwardDraft, TreeChange, TreeItem,
    exchange::{
        ConnectionsPart, ExchangeRow, ExportedConnection, ExportedMark, RowStatus, SOURCE_PREFIX,
    },
};

impl Engine {
    /// The jump hosts and the forwards of the connections part, against `rows`, its rows.
    pub(super) async fn exchange_links(
        &self,
        part: &ConnectionsPart,
        rows: &mut [ExchangeRow],
        dry_run: bool,
    ) -> Result<(), EngineError> {
        let db = self.db()?;
        let own = format!("{SOURCE_PREFIX}{}", db.installation_id().await?);
        let here = db.connection_origins().await?;
        let in_file: HashSet<(&str, &str)> = part
            .connections
            .iter()
            .map(|entry| (entry.source.as_str(), entry.key.as_str()))
            .collect();
        for entry in &part.connections {
            let key = format!("connection:{}/{}", entry.source, entry.key);
            let Some(row) = rows.iter_mut().find(|row| row.key == key) else {
                continue;
            };
            let jump = entry.jump.as_ref();
            let known = jump.is_none_or(|jump| {
                in_file.contains(&(jump.source.as_str(), jump.key.as_str()))
                    || resolve(&here, &own, jump).is_some()
            });
            if !known && row.warning.is_none() {
                row.warning = Some("jump_missing".to_owned());
            }
            let written = matches!(row.status, RowStatus::Created | RowStatus::Updated)
                || (row.selected && row.status == RowStatus::Exists);
            if dry_run || !written {
                continue;
            }
            let mark = ExportedMark {
                source: entry.source.clone(),
                key: entry.key.clone(),
            };
            let Some(id) = resolve(&here, &own, &mark) else {
                continue;
            };
            if let Some(jump) = jump {
                let target = resolve(&here, &own, jump).filter(|jump| *jump != id);
                if !self.set_exchanged_jump(id, target).await {
                    row.warning = Some("jump_missing".to_owned());
                }
            }
            self.add_exchanged_forwards(id, entry).await?;
        }
        Ok(())
    }

    /// Gives the connection `id` the jump host `jump`; whether it has one then.
    async fn set_exchanged_jump(&self, id: ConnectionId, jump: Option<ConnectionId>) -> bool {
        let Some(jump) = jump else {
            return false;
        };
        let Ok(db) = self.db() else {
            return false;
        };
        if db
            .connection(id)
            .await
            .is_ok_and(|connection| connection.jump_id == Some(jump))
        {
            return true;
        }
        let patch = ConnectionPatch {
            jump_id: Some(jump.0),
            ..ConnectionPatch::default()
        };
        match db.update_connection(id, &patch).await {
            Ok(_) => {
                self.announce(TreeItem::Connection(id), TreeChange::Updated);
                true
            }
            Err(error) => {
                tracing::warn!(connection = %id, code = error.code(), "an imported jump host was not set");
                false
            }
        }
    }

    /// Adds the forwards of `entry` that the connection `id` does not have.
    async fn add_exchanged_forwards(
        &self,
        id: ConnectionId,
        entry: &ExportedConnection,
    ) -> Result<(), EngineError> {
        if entry.forwards.is_empty() {
            return Ok(());
        }
        let db = self.db()?;
        let present = db.forwards(id).await?;
        for forward in &entry.forwards {
            let there = present.iter().any(|stored| {
                stored.bind_port == forward.bind_port
                    && stored
                        .target_host
                        .eq_ignore_ascii_case(&forward.target_host)
                    && stored.target_port == forward.target_port
            });
            if there {
                continue;
            }
            let draft = ForwardDraft {
                bind_address: forward.bind_address.clone(),
                bind_port: i64::from(forward.bind_port),
                target_host: forward.target_host.clone(),
                target_port: i64::from(forward.target_port),
                auto_start: forward.auto_start,
                name: forward.name.clone(),
            };
            if let Err(error) = db.create_forward(id, &draft).await {
                tracing::warn!(connection = %id, code = error.code(), "an imported port forward was not added");
            }
        }
        Ok(())
    }
}

/// The connection here that `mark` names: one an import marked with it, or, for this
/// installation's own mark, the connection of that id.
fn resolve(here: &[ConnectionOrigin], own: &str, mark: &ExportedMark) -> Option<ConnectionId> {
    here.iter()
        .find(|origin| {
            origin.source.as_deref() == Some(mark.source.as_str())
                && origin.key.as_deref() == Some(mark.key.as_str())
        })
        .map(|origin| origin.id)
        .or_else(|| {
            let id = ConnectionId(mark.key.parse().ok()?);
            (mark.source == own && here.iter().any(|origin| origin.id == id)).then_some(id)
        })
}

#[cfg(test)]
#[path = "exchange_links_tests.rs"]
mod tests;
