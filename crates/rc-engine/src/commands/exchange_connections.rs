//! The rows of an import's connections part (RC-080-19): the login scripts first, then the
//! connections that run them.
//!
//! A script is known by its name (ignoring ASCII case) and checked as the editor checks it. Its
//! steps name its secrets by the ids of the machine it came from; they are named again by label
//! with the ids here. A connection is known by its mark (`source`, `key`): this installation's
//! own mark names the connection by id, any other the connection an earlier import marked with
//! it - so a second import, and a connection that travels there and back, update instead of
//! adding twice. A new one goes into its folders, made where missing; a changed one keeps its
//! place. A change of server for a connection with stored secrets waits for the person's tick:
//! its secrets would otherwise go to another server. Secrets go into the vault, never anywhere
//! else. A colour theme of a connection that is not here, nor among the file's themes, for its
//! mode is left out with a hint (`theme_missing`, RC-100-51); the row stays chosen as it was.

#[path = "exchange_connections_values.rs"]
mod values;

use std::collections::{HashMap, HashSet};

use rc_vault::SecretString;

use self::values::{endpoint, fit_themes, folded, ids_by_label, patch_of, renamed, same, walk};
use super::{
    exchange_import::{choose, written},
    exchange_import_rows::{joined, refused, row},
};
use crate::{
    Connection, ConnectionDraft, ConnectionId, Engine, EngineError, FolderId, LoginScriptDraft,
    LoginScriptId, LoginScriptInfo, LoginScriptPatch, Outcome, SecretKind, SecretParam,
    SecretSourceRef, TreeChange, TreeItem,
    exchange::{
        ConnectionsPart, ExchangePart, ExchangeRow, ExportedConnection, ExportedScript,
        ExportedTheme, RowStatus, SOURCE_PREFIX,
    },
};

impl Engine {
    /// The scripts, then the connections; `incoming` the file's themes a dry run has not written.
    pub(super) async fn exchange_connections(
        &self,
        part: &ConnectionsPart,
        incoming: &[ExportedTheme],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let (mut rows, scripts) = self
            .exchange_scripts(&part.login_scripts, only, dry_run)
            .await?;
        let db = self.db()?;
        let own = format!("{SOURCE_PREFIX}{}", db.installation_id().await?);
        let tree = db.connection_tree().await?;
        let mut listed: Vec<(Vec<String>, Connection)> = tree
            .connections
            .iter()
            .map(|connection| (Vec::new(), connection.clone()))
            .collect();
        let mut folders = HashMap::new();
        walk(&tree.folders, &[], &mut listed, &mut folders);
        let marks = db.connection_origins().await?;
        let mut taken: HashSet<(Vec<String>, String)> = listed
            .iter()
            .map(|(path, connection)| (path.clone(), folded(&connection.name)))
            .collect();
        let mut seen = HashSet::new();
        let themes = crate::settings::themes::load(db).await;
        for entry in &part.connections {
            let (draft, unthemed) = fit_themes(&entry.connection, &themes, incoming);
            let mut row = row(
                format!("connection:{}/{}", entry.source, entry.key),
                ExchangePart::Connections,
                joined(&entry.folders, &entry.connection.name),
            );
            row.new = Some(endpoint(&entry.connection));
            row.secrets = entry.has_secrets();
            let path: Vec<String> = entry.folders.iter().map(|name| folded(name)).collect();
            let marked = entry.source.starts_with(SOURCE_PREFIX)
                && entry.source.len() <= 80
                && !entry.key.is_empty()
                && entry.key.len() <= 40
                && seen.insert(row.key.clone());
            let checked = rc_db::connections::check_draft(&entry.connection);
            if !marked || path.iter().any(String::is_empty) || path.len() > crate::MAX_DEPTH {
                row.status = refused("exchange_file_invalid");
                rows.push(row);
                continue;
            }
            if let Err(error) = checked {
                row.status = refused(error.code());
                rows.push(row);
                continue;
            }
            let there = if entry.source == own {
                entry
                    .key
                    .parse::<i64>()
                    .ok()
                    .and_then(|id| listed.iter().find(|(_, c)| c.id == ConnectionId(id)))
            } else {
                marks
                    .iter()
                    .find(|mark| {
                        mark.source.as_deref() == Some(entry.source.as_str())
                            && mark.key.as_deref() == Some(entry.key.as_str())
                    })
                    .and_then(|mark| listed.iter().find(|(_, c)| c.id == mark.id))
            };
            let script_id = entry
                .login_script
                .as_deref()
                .and_then(|name| scripts.get(&folded(name)).copied());
            row.status = match there {
                None if !taken.insert((path.clone(), folded(&entry.connection.name))) => {
                    refused("name_taken")
                }
                None => RowStatus::New,
                Some((_, connection)) => {
                    let runs = connection.login_script_id == script_id.map(|id| id.0);
                    if same(connection, &draft) && runs {
                        RowStatus::Exists
                    } else {
                        let draft = super::exchange_export::draft_of(connection);
                        row.old = Some(endpoint(&draft));
                        let stored = connection.secrets;
                        let moved = endpoint(&draft) != endpoint(&entry.connection);
                        if moved && (stored.has_password || stored.has_key || stored.has_passphrase)
                        {
                            row.warning = Some("endpoint_with_secrets".to_owned());
                        }
                        RowStatus::Changed
                    }
                }
            };
            // A source's secret goes to this server at connect: ticked by a person (RC-100-34).
            if row.warning.is_none() && !entry.sources.is_empty() {
                row.warning = Some("secret_sources".to_owned());
            }
            // A key file on a share is never read (`local_key_path`), and still said.
            let key_path = entry.connection.key_path.as_deref();
            if row.warning.is_none()
                && key_path.is_some_and(|path| crate::network_path(path).is_some())
            {
                row.warning = Some("network_location".to_owned());
            }
            choose(&mut row, only);
            // Only a hint: the connection comes in, without the theme.
            if unthemed && row.warning.is_none() {
                row.warning = Some("theme_missing".to_owned());
            }
            if !dry_run && row.selected {
                let there = there.map(|(_, connection)| connection);
                let result = self
                    .write_connection(entry, &draft, there, script_id, &path, &mut folders)
                    .await;
                let done = if there.is_some() {
                    RowStatus::Updated
                } else {
                    RowStatus::Created
                };
                row.status = match result {
                    Err(error @ (EngineError::VaultLocked | EngineError::Vault(_))) => {
                        return Err(error);
                    }
                    other => written(other, done),
                };
            }
            rows.push(row);
        }
        Ok(rows)
    }

    /// Writes one chosen connection, with the fields of `draft`: made in its folders, or
    /// updated; its script; its secrets.
    async fn write_connection(
        &self,
        entry: &ExportedConnection,
        draft: &ConnectionDraft,
        there: Option<&Connection>,
        script: Option<LoginScriptId>,
        path: &[String],
        folders: &mut HashMap<Vec<String>, FolderId>,
    ) -> Result<(), EngineError> {
        let db = self.db()?;
        let connection = match there {
            None => {
                let folder = self.exchange_folder(&entry.folders, path, folders).await?;
                let made = db
                    .create_marked_connection(folder, draft, &entry.source, &entry.key)
                    .await?;
                self.announce(TreeItem::Connection(made.id), TreeChange::Created);
                made
            }
            Some(there) if !same(there, draft) => {
                let updated = db.update_connection(there.id, &patch_of(draft)).await?;
                self.forget_remote(there.id).await;
                self.announce(TreeItem::Connection(there.id), TreeChange::Updated);
                updated
            }
            Some(there) => there.clone(),
        };
        let id = connection.id;
        if connection.login_script_id != script.map(|script| script.0) {
            db.set_connection_login_script(id, script).await?;
            self.announce(TreeItem::Connection(id), TreeChange::Updated);
        }
        if let Some(password) = &entry.password {
            self.set_secret(
                id,
                SecretKind::Password,
                &SecretParam::from(password.expose()),
            )
            .await?;
        }
        match (&entry.private_key, &entry.passphrase) {
            (Some(key), passphrase) => {
                let passphrase = passphrase
                    .as_ref()
                    .map(|passphrase| SecretString::from(passphrase.expose()));
                self.store_private_key(id, SecretString::from(key.expose()), passphrase)
                    .await?;
            }
            (None, Some(passphrase)) => {
                let value = SecretParam::from(passphrase.expose());
                self.set_secret(id, SecretKind::Passphrase, &value).await?;
            }
            (None, None) => {}
        }
        for source in &entry.sources {
            let reference = SecretSourceRef {
                connection_id: id,
                kind: source.kind,
                source: source.source.clone(),
                entry_id: source.entry_id.clone(),
                entry_label: source.entry_label.clone(),
            };
            crate::secret_sources::check_reference(&reference)?;
            db.set_secret_source(&reference).await?;
        }
        Ok(())
    }

    /// The folder `names` (folded as `path`) from the root down, made where missing; `None` for
    /// the root.
    async fn exchange_folder(
        &self,
        names: &[String],
        path: &[String],
        folders: &mut HashMap<Vec<String>, FolderId>,
    ) -> Result<Option<FolderId>, EngineError> {
        let mut parent = None;
        for depth in 1..=path.len() {
            if let Some(id) = folders.get(&path[..depth]) {
                parent = Some(*id);
                continue;
            }
            let made = self.db()?.create_folder(parent, &names[depth - 1]).await?;
            self.announce(TreeItem::Folder(made.id), TreeChange::Created);
            folders.insert(path[..depth].to_vec(), made.id);
            parent = Some(made.id);
        }
        Ok(parent)
    }

    /// The scripts against those here, and every script name (folded) to its id here.
    async fn exchange_scripts(
        &self,
        scripts: &[ExportedScript],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<(Vec<ExchangeRow>, HashMap<String, LoginScriptId>), EngineError> {
        let here: Vec<LoginScriptInfo> = self
            .db()?
            .login_scripts()
            .await?
            .into_iter()
            .map(LoginScriptInfo::from_record)
            .collect::<Result<_, _>>()?;
        let mut ids: HashMap<String, LoginScriptId> = here
            .iter()
            .map(|script| (folded(&script.name), script.id))
            .collect();
        let mut seen = HashSet::new();
        let mut rows = Vec::with_capacity(scripts.len());
        for script in scripts {
            let mut row = row(
                format!("script:{}", folded(&script.name)),
                ExchangePart::Connections,
                script.name.clone(),
            );
            row.new = Some(format!("{} steps", script.steps.len()));
            row.secrets = script.secrets.iter().any(|secret| secret.value.is_some());
            let checked = rc_remote::login_script::LoginScript {
                steps: script.steps.clone(),
                step_timeout_ms: script.step_timeout_ms,
                total_timeout_ms: script.total_timeout_ms,
            }
            .validate();
            if folded(&script.name).is_empty() || !seen.insert(row.key.clone()) {
                row.status = refused("exchange_file_invalid");
                rows.push(row);
                continue;
            }
            if let Err(invalid) = checked {
                row.status = refused(invalid.code());
                rows.push(row);
                continue;
            }
            let labels: HashMap<i64, String> = script
                .secrets
                .iter()
                .map(|secret| (secret.id, secret.label.clone()))
                .collect();
            let there = here
                .iter()
                .find(|there| folded(&there.name) == folded(&script.name));
            row.status = match there {
                None => RowStatus::New,
                Some(there)
                    if renamed(&script.steps, &labels, &ids_by_label(there)) == there.steps
                        && there.step_timeout_ms == script.step_timeout_ms
                        && there.total_timeout_ms == script.total_timeout_ms =>
                {
                    RowStatus::Exists
                }
                Some(_) => RowStatus::Changed,
            };
            choose(&mut row, only);
            if !dry_run && row.selected {
                let result = self.write_script(script, there, &labels).await;
                if let Ok(id) = &result {
                    ids.insert(folded(&script.name), *id);
                }
                let done = if there.is_some() {
                    RowStatus::Updated
                } else {
                    RowStatus::Created
                };
                row.status = match result {
                    Err(error @ (EngineError::VaultLocked | EngineError::Vault(_))) => {
                        return Err(error);
                    }
                    other => written(other.map(|_| ()), done),
                };
            }
            rows.push(row);
        }
        Ok((rows, ids))
    }

    /// Writes one chosen script: made or updated, its secrets stored, its steps naming them by
    /// their ids here.
    async fn write_script(
        &self,
        script: &ExportedScript,
        there: Option<&LoginScriptInfo>,
        labels: &HashMap<i64, String>,
    ) -> Result<LoginScriptId, EngineError> {
        let id = match there {
            Some(there) => there.id,
            None => {
                let draft = LoginScriptDraft {
                    name: script.name.clone(),
                    steps: script.steps.clone(),
                    step_timeout_ms: Some(script.step_timeout_ms),
                    total_timeout_ms: Some(script.total_timeout_ms),
                };
                match self.create_login_script(&draft).await? {
                    Outcome::LoginScript { script } => script.id,
                    _ => return Err(EngineError::DatabaseUnavailable),
                }
            }
        };
        let mut info = None;
        for secret in &script.secrets {
            if let Some(value) = &secret.value {
                let stored = self
                    .set_login_script_secret(id, &secret.label, &SecretParam::from(value.expose()))
                    .await?;
                if let Outcome::LoginScript { script } = stored {
                    info = Some(script);
                }
            }
        }
        let info = match (info, there) {
            (Some(info), _) => info,
            (None, Some(there)) => there.clone(),
            (None, None) => match self.get_login_script(id).await? {
                Outcome::LoginScript { script } => script,
                _ => return Err(EngineError::DatabaseUnavailable),
            },
        };
        let steps = renamed(&script.steps, labels, &ids_by_label(&info));
        let patch = LoginScriptPatch {
            steps: Some(steps),
            step_timeout_ms: Some(script.step_timeout_ms),
            total_timeout_ms: Some(script.total_timeout_ms),
            ..LoginScriptPatch::default()
        };
        self.update_login_script(id, &patch).await?;
        Ok(id)
    }
}

#[cfg(test)]
#[path = "exchange_connections_tests.rs"]
mod tests;
