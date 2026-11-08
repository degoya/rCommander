//! What an ssh_config import sets once every host is written (RC-110-03): the jump host of
//! `ProxyJump <alias>`, and the port forwards of `LocalForward`.
//!
//! The alias is looked up among the hosts of this import and the connections of earlier
//! ssh_config imports (their mark's key is the alias in lower case); one that is neither stays
//! unset, and the report says so by leaving `jump_id` empty. Under MCP the jump host is an
//! endpoint change like any other (`endpoint_change_needs_gui` for a connection that holds a
//! secret, logged), and no forward is created: a listening port is a person's decision, so an
//! agent's import only lists them (`forwards_gui_only`). Forwards already there (same port and
//! target) are not added twice.

use std::collections::HashMap;

use rc_db::connections::{EndpointRule, ImportStatus};

use crate::{
    ConnectionId, ConnectionPatch, Engine, EngineError, ForwardDraft, TreeChange, TreeItem,
    import::{
        ssh_config::SshConfigImport,
        ssh_config_apply::{SshConfigReport, key_of, single_jump},
    },
    read_guard,
};

/// The prefix of the marks an ssh_config import writes.
const SSH_CONFIG_SOURCE: &str = "ssh_config:";

impl Engine {
    /// Sets the jump hosts and adds the forwards of the hosts `report` says were written.
    pub(crate) async fn link_imported(
        &self,
        found: &SshConfigImport,
        report: &mut SshConfigReport,
    ) -> Result<(), EngineError> {
        let db = self.db()?;
        let mut ids: HashMap<String, ConnectionId> = found
            .hosts
            .iter()
            .zip(&report.hosts)
            .filter_map(|(host, row)| written(&row.status).map(|id| (key_of(&host.alias), id)))
            .collect();
        for origin in db.connection_origins().await? {
            if let (Some(source), Some(key)) = (&origin.source, &origin.key)
                && source.starts_with(SSH_CONFIG_SOURCE)
            {
                ids.entry(key.clone()).or_insert(origin.id);
            }
        }
        let mcp = read_guard::from_mcp();
        for (host, row) in found.hosts.iter().zip(&mut report.hosts) {
            let Some(id) = written(&row.status) else {
                continue;
            };
            let jump = host
                .proxy_jump
                .as_deref()
                .and_then(single_jump)
                .and_then(|alias| ids.get(&key_of(alias)).copied())
                .filter(|jump| *jump != id);
            if let Some(jump) = jump {
                row.jump_id = self.set_imported_jump(id, jump, mcp).await;
            }
            if !mcp {
                self.add_imported_forwards(id, host).await?;
            }
        }
        Ok(())
    }

    /// Gives the connection `id` the jump host `jump`; the jump host it has then, `None` when
    /// the repository refused (a cycle, a chain too long, secrets held under MCP).
    async fn set_imported_jump(
        &self,
        id: ConnectionId,
        jump: ConnectionId,
        mcp: bool,
    ) -> Option<ConnectionId> {
        let db = self.db().ok()?;
        if db.connection(id).await.ok()?.jump_id == Some(jump) {
            return Some(jump);
        }
        let patch = ConnectionPatch {
            jump_id: Some(jump.0),
            ..ConnectionPatch::default()
        };
        let rule = if mcp {
            EndpointRule::KeepWhileSecretsHeld
        } else {
            EndpointRule::Free
        };
        match db.update_connection_with(id, &patch, rule).await {
            Ok(_) => {
                self.announce(TreeItem::Connection(id), TreeChange::Updated);
                Some(jump)
            }
            Err(error) => {
                tracing::warn!(connection = %id, code = error.code(), "an imported jump host was not set");
                None
            }
        }
    }

    /// Adds the `LocalForward` lines of `host` to the connection `id`, each that is not there.
    async fn add_imported_forwards(
        &self,
        id: ConnectionId,
        host: &crate::import::ssh_config::ImportedHost,
    ) -> Result<(), EngineError> {
        if host.local_forwards.is_empty() {
            return Ok(());
        }
        let db = self.db()?;
        let present = db.forwards(id).await?;
        for spec in &host.local_forwards {
            let there = present.iter().any(|forward| {
                forward.bind_port == spec.port
                    && forward.target_host.eq_ignore_ascii_case(&spec.host)
                    && forward.target_port == spec.host_port
            });
            if there {
                continue;
            }
            let draft = ForwardDraft {
                bind_address: spec.bind.clone().unwrap_or_default(),
                bind_port: i64::from(spec.port),
                target_host: spec.host.clone(),
                target_port: i64::from(spec.host_port),
                auto_start: false,
                name: None,
            };
            if let Err(error) = db.create_forward(id, &draft).await {
                tracing::warn!(connection = %id, code = error.code(), "an imported port forward was not added");
            }
        }
        Ok(())
    }
}

/// The connection a status names once it is in the tree.
fn written(status: &ImportStatus) -> Option<ConnectionId> {
    match status {
        ImportStatus::Created { id }
        | ImportStatus::Updated { id }
        | ImportStatus::Unchanged { id } => Some(*id),
        ImportStatus::New | ImportStatus::Update { .. } | ImportStatus::Refused { .. } => None,
    }
}

#[cfg(test)]
#[path = "ssh_import_links_tests.rs"]
mod tests;
