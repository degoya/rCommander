//! The `connections.*` and `folders.*` commands: the connection tree in `rcommander.db`.
//!
//! Each one is a call to the repository, which validates and writes in one transaction, then
//! one [`EngineEvent::ConnectionTree`] for the node it changed. A changed or deleted connection
//! drops its open remote session.
//!
//! Stored secrets are bound to their endpoint: an MCP command never changes the host, port,
//! kind or user of a connection that has one - or runs a login script that sends one - or a
//! test, connect or terminal afterwards would hand the decrypted secret to whatever host an agent
//! named. A person changes them in the window.

use rc_db::connections::{Connection, EndpointRule};

use crate::{
    ConnectionDraft, ConnectionId, ConnectionPatch, Engine, EngineError, EngineEvent, FolderId,
    LoginScriptId, Outcome, TreeChange, TreeItem, read_guard,
};

/// The settings key of the connection the window logged in to last (RC-050-15).
pub(crate) const LAST_USED_KEY: &str = "connections.last_used";

/// What a change may do to a stored connection's endpoint: an MCP caller keeps it while the
/// connection holds a secret, judged by `rc-db` inside the change's transaction (RC-080-50).
pub(super) fn endpoint_rule() -> EndpointRule {
    if read_guard::from_mcp() {
        EndpointRule::KeepWhileSecretsHeld
    } else {
        EndpointRule::Free
    }
}

/// Whether `connection` holds a stored secret (any of `rc_db::SECRET_COLUMNS`).
fn has_secrets(connection: &Connection) -> bool {
    let flags = connection.secrets;
    flags.has_password || flags.has_key || flags.has_passphrase
}

/// Whether `patch` changes where `connection` connects: its host, port, kind or user.
fn moves_endpoint(connection: &Connection, patch: &ConnectionPatch) -> bool {
    patch
        .host
        .as_ref()
        .is_some_and(|host| *host != connection.host)
        || patch
            .port
            .is_some_and(|port| port != i64::from(connection.port))
        || patch.kind.is_some_and(|kind| kind != connection.kind)
        || patch
            .username
            .as_ref()
            .is_some_and(|user| Some(user) != connection.username.as_ref())
}

impl Engine {
    /// Whether `connection` holds a stored secret, takes one from a shared credential (RC-050-12)
    /// or a source (RC-100-34), or runs a login script that sends one: either way a new endpoint would receive it (RC-080-12). A
    /// failing look counts as holding one.
    pub(crate) async fn holds_secrets(&self, connection: &Connection) -> bool {
        // A shared credential's secrets go wherever the connection points (RC-050-12).
        if has_secrets(connection) || connection.credential_id.is_some() {
            return true;
        }
        let sourced = match self.db() {
            Ok(db) => db
                .secret_sources(connection.id)
                .await
                .map_or(true, |references| !references.is_empty()),
            Err(_) => true,
        };
        if sourced {
            return true;
        }
        match connection.login_script_id {
            Some(id) => self.script_sends_secret(LoginScriptId(id)).await,
            None => false,
        }
    }

    /// Refuses `patch` when it moves the endpoint of connection `id` and the connection holds a
    /// secret: an MCP call's change, judged under `secret_writes`.
    pub(super) async fn refuse_endpoint_move(
        &self,
        id: ConnectionId,
        patch: &ConnectionPatch,
    ) -> Result<(), EngineError> {
        let current = self.db()?.connection(id).await?;
        if moves_endpoint(&current, patch) && self.holds_secrets(&current).await {
            return Err(EngineError::EndpointChangeNeedsGui { id: id.0 });
        }
        Ok(())
    }

    pub(super) fn announce(&self, item: TreeItem, change: TreeChange) {
        self.publish(EngineEvent::ConnectionTree { item, change });
    }

    /// `connections.list`: the whole tree, with the connection used last.
    pub(crate) async fn list_connections(&self) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let mut tree = db.connection_tree().await?;
        // A value that does not read is no reason to fail the list.
        tree.last_used = db.get(LAST_USED_KEY).await.ok().flatten();
        Ok(Outcome::ConnectionTree { tree })
    }

    /// Remembers `id` as the connection used last, when the window logs in to it (never an
    /// agent's call); a failed write only costs the manager its starting row.
    pub(crate) async fn remember_used(&self, id: ConnectionId) {
        if read_guard::from_mcp() {
            return;
        }
        let Ok(db) = self.db() else {
            return;
        };
        if let Err(error) = db.set(LAST_USED_KEY, &id).await {
            tracing::warn!(%error, "the connection used last is not remembered");
        }
    }

    /// `connections.get`.
    pub(crate) async fn get_connection(&self, id: ConnectionId) -> Result<Outcome, EngineError> {
        let connection = self.db()?.connection(id).await?;
        Ok(Outcome::Connection { connection })
    }

    /// `connections.create`.
    pub(crate) async fn create_connection(
        &self,
        folder_id: Option<FolderId>,
        draft: &ConnectionDraft,
    ) -> Result<Outcome, EngineError> {
        let connection = self.db()?.create_connection(folder_id, draft).await?;
        self.announce(TreeItem::Connection(connection.id), TreeChange::Created);
        Ok(Outcome::Connection { connection })
    }

    /// `connections.update`; under MCP never the endpoint of a connection with secrets. The look
    /// runs inside the update's own transaction (`EndpointRule::KeepWhileSecretsHeld`), so no
    /// secret stored by another process lands between them, and under the engine's secret
    /// writes (`secret_writes`), so a store of this one finishes first (RC-080-50).
    pub(crate) async fn update_connection(
        &self,
        id: ConnectionId,
        patch: &ConnectionPatch,
    ) -> Result<Outcome, EngineError> {
        let connection = {
            let _writes = if read_guard::from_mcp() {
                Some(self.inner.secret_writes.lock().await)
            } else {
                None
            };
            let rule = endpoint_rule();
            self.db()?.update_connection_with(id, patch, rule).await?
        };
        self.forget_remote(id).await;
        self.announce(TreeItem::Connection(id), TreeChange::Updated);
        Ok(Outcome::Connection { connection })
    }

    /// `connections.move`.
    pub(crate) async fn move_connection(
        &self,
        id: ConnectionId,
        folder_id: Option<FolderId>,
        sort_order: Option<i64>,
    ) -> Result<Outcome, EngineError> {
        let connection = self
            .db()?
            .move_connection(id, folder_id, sort_order)
            .await?;
        self.announce(TreeItem::Connection(id), TreeChange::Moved);
        Ok(Outcome::Connection { connection })
    }

    /// `connections.delete`: its stored secrets go with it.
    pub(crate) async fn delete_connection(&self, id: ConnectionId) -> Result<Outcome, EngineError> {
        self.db()?.delete_connection(id).await?;
        self.forget_remote(id).await;
        let item = TreeItem::Connection(id);
        self.announce(item, TreeChange::Deleted);
        self.announce_if_vault_unused().await;
        Ok(Outcome::Deleted {
            item,
            folders: 0,
            connections: 1,
        })
    }

    /// `folders.create`.
    pub(crate) async fn create_folder(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
    ) -> Result<Outcome, EngineError> {
        let folder = self.db()?.create_folder(parent_id, name).await?;
        self.announce(TreeItem::Folder(folder.id), TreeChange::Created);
        Ok(Outcome::Folder { folder })
    }

    /// `folders.rename`.
    pub(crate) async fn rename_folder(
        &self,
        id: FolderId,
        name: &str,
    ) -> Result<Outcome, EngineError> {
        let folder = self.db()?.rename_folder(id, name).await?;
        self.announce(TreeItem::Folder(id), TreeChange::Updated);
        Ok(Outcome::Folder { folder })
    }

    /// `folders.move`.
    pub(crate) async fn move_folder(
        &self,
        id: FolderId,
        parent_id: Option<FolderId>,
        sort_order: Option<i64>,
    ) -> Result<Outcome, EngineError> {
        let folder = self.db()?.move_folder(id, parent_id, sort_order).await?;
        self.announce(TreeItem::Folder(id), TreeChange::Moved);
        Ok(Outcome::Folder { folder })
    }

    /// `folders.delete`: an empty folder, or with `recursive` everything inside it too.
    pub(crate) async fn delete_folder(
        &self,
        id: FolderId,
        recursive: bool,
    ) -> Result<Outcome, EngineError> {
        let removed = self.db()?.delete_folder(id, recursive).await?;
        if removed.connections > 0 {
            self.forget_remotes().await;
        }
        let item = TreeItem::Folder(id);
        self.announce(item, TreeChange::Deleted);
        self.announce_if_vault_unused().await;
        Ok(Outcome::Deleted {
            item,
            folders: removed.folders,
            connections: removed.connections,
        })
    }
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
