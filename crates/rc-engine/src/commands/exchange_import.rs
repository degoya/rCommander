//! `config.import` (RC-080-19): an `.rcexport` file read as hostile input, its rows compared with
//! this machine's, and with `dry_run` off the chosen ones written.
//!
//! The whole file is read and checked before one row is compared: at most
//! [`MAX_FILE_BYTES`], this format and version, a sealed one opened with its export password -
//! a wrong password or a changed byte is refused alike - and every part within its caps. A
//! refusal writes nothing. Then each row on its own: a row that does not hold up (an unknown
//! setting, a host key that does not parse, a path on a server) is `refused` and the others go
//! on. A row that changes a trust decision or a protected setting, or points a connection with
//! stored secrets at another server, carries a warning and is written only when the person
//! ticks it (`only`). Secrets go straight into the vault, which is asked for before anything is
//! written when a chosen row brings one.
//!
//! The parts' rows are `exchange_import_rows.rs` (settings, trust), `exchange_connections.rs`
//! and `exchange_places.rs` (hotlist, tabs).

use rc_vault::{ExposeSecret as _, MasterPassword, VaultError};
use zeroize::Zeroizing;

use crate::{
    ConfigImportRequest, Engine, EngineError, Outcome,
    exchange::{
        Content, ExchangeError, ExchangeReport, ExchangeRow, ExportFile, ExportParts,
        MAX_FILE_BYTES, RowStatus,
    },
};

/// Whether `row` is written: named in `only`; with no `only` when it is new or changed and
/// carries no warning.
pub(super) fn choose(row: &mut ExchangeRow, only: Option<&[String]>) {
    let choosable = row.status.choosable(row.secrets);
    row.selected = choosable
        && match only {
            Some(keys) => keys.contains(&row.key),
            None => row.warning.is_none() && row.status != RowStatus::Exists,
        };
}

/// The status after the write of a chosen row: `done`, or the failure's code.
pub(super) fn written(result: Result<(), EngineError>, done: RowStatus) -> RowStatus {
    match result {
        Ok(()) => done,
        Err(error) => RowStatus::Failed {
            code: error.code().to_owned(),
        },
    }
}

/// A value as a row shows it: a text without its quotes, at most 200 characters.
pub(super) fn shown(value: &serde_json::Value) -> String {
    let text = match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    text.chars().take(200).collect()
}

impl Engine {
    /// `config.import`: `GuiOnly`.
    pub(crate) async fn import_config(
        &self,
        request: &ConfigImportRequest,
    ) -> Result<Outcome, EngineError> {
        let bytes = read_file(&request.path).await?;
        let file = ExportFile::parse(&bytes)?;
        drop(bytes);
        let encrypted = file.sealed.is_some();
        let created_at = file.created_at.clone();
        let content = match (file.content, file.sealed) {
            (Some(content), _) => content,
            (None, Some(sealed)) => {
                let password = request
                    .password
                    .as_ref()
                    .filter(|password| !password.is_empty())
                    .ok_or(ExchangeError::PasswordNeeded)?;
                let password = MasterPassword::from(password.secret().clone());
                let opened = rc_vault::open_export(&password, &sealed.sealed()?)
                    .await
                    .map_err(|error| match error {
                        VaultError::WrongPassword => ExchangeError::PasswordWrong.into(),
                        VaultError::BadParameters | VaultError::Malformed => {
                            ExchangeError::FileInvalid {
                                reason: "the seal's parameters are not usable".to_owned(),
                            }
                            .into()
                        }
                        other => EngineError::from(other),
                    })?;
                Content::parse(opened.expose_secret())?
            }
            // `ExportFile::parse` refuses a file without either.
            (None, None) => {
                return Err(ExchangeError::FileInvalid {
                    reason: "no content".to_owned(),
                }
                .into());
            }
        };
        content.check()?;
        let only = request.only.as_deref();
        let mut rows = self.exchange_rows(&content, only, true).await?;
        if !request.dry_run {
            // The vault is asked before the first write, never halfway through.
            if rows.iter().any(|row| row.selected && row.secrets) {
                self.vault_handle().await?.touch()?;
            }
            rows = self.exchange_rows(&content, only, false).await?;
        }
        let parts = ExportParts {
            settings: content.settings.is_some(),
            connections: content.connections.is_some(),
            trust: content.trust.is_some(),
            hotlist: content.hotlist.is_some(),
        };
        tracing::debug!(
            dry_run = request.dry_run,
            encrypted,
            rows = rows.len(),
            "an export read"
        );
        Ok(Outcome::ConfigImported(ExchangeReport {
            dry_run: request.dry_run,
            encrypted,
            created_at,
            secrets: content.secrets,
            parts,
            rows,
        }))
    }

    /// Every part's rows in the file's order: settings, login scripts and connections, host keys
    /// and certificates, hotlist and tabs; written unless `dry_run`.
    async fn exchange_rows(
        &self,
        content: &Content,
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let mut rows = Vec::new();
        if let Some(settings) = &content.settings {
            rows.extend(self.exchange_settings(settings, only, dry_run).await?);
        }
        if let Some(part) = &content.connections {
            rows.extend(self.exchange_connections(part, only, dry_run).await?);
        }
        if let Some(part) = &content.trust {
            rows.extend(
                self.exchange_host_keys(&part.host_keys, only, dry_run)
                    .await?,
            );
            rows.extend(
                self.exchange_certificates(&part.certificates, only, dry_run)
                    .await?,
            );
        }
        if let Some(part) = &content.hotlist {
            rows.extend(self.exchange_hotlist(&part.items, only, dry_run).await?);
            rows.extend(self.exchange_tabs(&part.tabs, only, dry_run).await?);
        }
        Ok(rows)
    }
}

/// The file at `path`: a regular file of at most [`MAX_FILE_BYTES`], off the async threads
/// (`import::bounded_read`); the buffer is zeroized.
async fn read_file(path: &str) -> Result<Zeroizing<Vec<u8>>, EngineError> {
    use crate::import::bounded_read::{BoundedReadError, read_bounded};

    let owned = std::path::PathBuf::from(path);
    let read = tokio::task::spawn_blocking(move || read_bounded(&owned, MAX_FILE_BYTES))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()));
    let error = match read {
        Ok(Ok(bytes)) => return Ok(bytes),
        Ok(Err(BoundedReadError::TooLarge { max })) => ExchangeError::FileTooLarge { max },
        Ok(Err(BoundedReadError::NotAFile)) => ExchangeError::FileUnreadable {
            path: path.to_owned(),
            source: std::io::Error::other("not a regular file"),
        },
        Ok(Err(BoundedReadError::Io(source))) | Err(source) => ExchangeError::FileUnreadable {
            path: path.to_owned(),
            source,
        },
    };
    Err(error.into())
}

#[cfg(test)]
#[path = "exchange_import_tests.rs"]
mod tests;
