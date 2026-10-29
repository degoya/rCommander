//! The rows of an import's settings and trust parts (RC-080-19).
//!
//! A setting is one of the schema and checked against its row; a protected one (`mcp.*`,
//! `vault.*`, the programs F3 and F4 start, the export's encryption) and a path on a network
//! share (`rc_vfs::network_path`) wait for the person's tick. A host key is parsed again and its fingerprint computed here, never taken from the
//! file; a certificate's fingerprint must read as one. A key or a certificate that differs from
//! the one trusted for its host is a conflict with a warning: replaced only when the person
//! ticks it, never silently.

use std::collections::HashSet;

use rc_db::{HASHED_PREFIX, KnownHostSource, NewKnownHost, NewTrustedCertificate};
use rc_remote::hostkey::{OfferedKey, normalize_host};

use super::exchange_import::{choose, shown, written};
use crate::{
    Engine, EngineError, EngineEvent,
    exchange::{
        ExchangePart, ExchangeRow, ExportedCertificate, ExportedHostKey, ExportedSetting, RowStatus,
    },
    settings::{SettingKind, is_protected, setting, stored_setting},
};

/// A row of `part` named `name`, new until compared.
pub(super) fn row(key: String, part: ExchangePart, name: String) -> ExchangeRow {
    ExchangeRow {
        key,
        part,
        name,
        old: None,
        new: None,
        warning: None,
        secrets: false,
        selected: false,
        status: RowStatus::New,
    }
}

/// `folders` and `name` as one path, `/` between.
pub(super) fn joined(folders: &[String], name: &str) -> String {
    let mut parts = folders.to_vec();
    parts.push(name.to_owned());
    parts.join("/")
}

/// A row that is not imported, for `code`.
pub(super) fn refused(code: &str) -> RowStatus {
    RowStatus::Refused {
        code: code.to_owned(),
    }
}

/// Whether `host` is a host name as the trust store keeps it: lower case, one word.
fn plain_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && !host.chars().any(|c| c.is_whitespace() || c.is_control())
        && normalize_host(host) == host
}

/// Whether `text` is a SHA-256 fingerprint as `AB:CD:...` in upper-case hex.
fn certificate_fingerprint(text: &str) -> bool {
    text.len() == 95
        && text.split(':').count() == 32
        && text.split(':').all(|pair| {
            pair.len() == 2
                && pair
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        })
}

impl Engine {
    /// The settings against the schema and the values in force.
    pub(super) async fn exchange_settings(
        &self,
        settings: &[ExportedSetting],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let db = self.db()?;
        let mut seen = HashSet::new();
        let mut rows = Vec::with_capacity(settings.len());
        for entry in settings {
            let mut row = row(
                format!("setting:{}", entry.setting),
                ExchangePart::Settings,
                entry.setting.clone(),
            );
            row.new = Some(shown(&entry.value));
            let Some(spec) = setting(&entry.setting) else {
                row.status = refused("setting_unknown");
                rows.push(row);
                continue;
            };
            let value = match spec.check(&entry.value) {
                Ok(value) if seen.insert(spec.key) => value,
                Ok(_) => {
                    row.status = refused("exchange_file_invalid");
                    rows.push(row);
                    continue;
                }
                Err(_) => {
                    row.status = refused("setting_invalid");
                    rows.push(row);
                    continue;
                }
            };
            let current = stored_setting(db, spec.key).await;
            if current.as_ref() == Some(&value) {
                row.status = RowStatus::Exists;
            } else {
                row.status = RowStatus::Changed;
                row.old = current.as_ref().map(shown);
            }
            if is_protected(spec.key) {
                row.warning = Some("protected_setting".to_owned());
            }
            // A start folder on a share would be opened at the next start (RC-080-10's rule).
            let path = matches!(spec.kind, SettingKind::Path { .. });
            if path
                && value
                    .as_str()
                    .is_some_and(|text| crate::network_path(text).is_some())
            {
                row.warning = Some("network_location".to_owned());
            }
            choose(&mut row, only);
            if !dry_run && row.selected {
                let stored = self.set_protected_setting(spec.key, &value).await;
                row.status = written(stored.map(|_| ()), RowStatus::Updated);
            }
            rows.push(row);
        }
        Ok(rows)
    }

    /// The host keys against the trust store.
    pub(super) async fn exchange_host_keys(
        &self,
        keys: &[ExportedHostKey],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let db = self.db()?;
        let stored = db.known_hosts().await?;
        let mut seen = HashSet::new();
        let mut rows = Vec::with_capacity(keys.len());
        let mut changed = false;
        for key in keys {
            let mut row = row(
                format!("host_key:{}:{}:{}", key.host, key.port, key.algorithm),
                ExchangePart::Trust,
                format!("{}:{} {}", key.host, key.port, key.algorithm),
            );
            let hashed = key.host.starts_with(HASHED_PREFIX);
            let host_ok = if hashed {
                key.port == 0 && !key.host.chars().any(char::is_whitespace)
            } else {
                key.port > 0 && plain_host(&key.host)
            };
            let offered = match OfferedKey::from_openssh(&key.algorithm, &key.public_key) {
                Ok(offered) if host_ok && offered.algorithm == key.algorithm => offered,
                Ok(_) => {
                    row.status = refused("exchange_file_invalid");
                    rows.push(row);
                    continue;
                }
                Err(error) => {
                    row.status = refused(error.code());
                    rows.push(row);
                    continue;
                }
            };
            row.new = Some(offered.fingerprint.clone());
            if !seen.insert(row.key.clone()) {
                row.status = refused("exchange_file_invalid");
                rows.push(row);
                continue;
            }
            let there = stored.iter().find(|stored| {
                stored.host == key.host
                    && stored.port == key.port
                    && stored.algorithm == key.algorithm
            });
            row.status = match there {
                None => RowStatus::New,
                Some(there) if there.public_key == offered.public_key => RowStatus::Exists,
                Some(there) => {
                    row.old = Some(there.fingerprint_sha256.clone());
                    row.warning = Some("host_key_changed".to_owned());
                    RowStatus::Changed
                }
            };
            choose(&mut row, only);
            if !dry_run && row.selected {
                let new = NewKnownHost {
                    host: key.host.clone(),
                    port: key.port,
                    algorithm: offered.algorithm,
                    public_key: offered.public_key,
                    fingerprint_sha256: offered.fingerprint,
                    source: KnownHostSource::Import,
                };
                row.status = match there {
                    None => match db.add_known_host(&new).await? {
                        Some(_) => RowStatus::Created,
                        None => refused("host_key_stale"),
                    },
                    Some(there) => {
                        match db
                            .replace_known_host(there.id, &there.fingerprint_sha256, &new)
                            .await?
                        {
                            Some(_) => RowStatus::Updated,
                            None => refused("host_key_stale"),
                        }
                    }
                };
                changed |= matches!(row.status, RowStatus::Created | RowStatus::Updated);
            }
            rows.push(row);
        }
        if changed {
            self.publish(EngineEvent::KnownHostsChanged);
        }
        Ok(rows)
    }

    /// The certificates against the trusted ones.
    pub(super) async fn exchange_certificates(
        &self,
        certificates: &[ExportedCertificate],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let db = self.db()?;
        let stored = db.trusted_certificates().await?;
        let mut seen = HashSet::new();
        let mut rows = Vec::with_capacity(certificates.len());
        let mut changed = false;
        for cert in certificates {
            let mut row = row(
                format!("certificate:{}:{}", cert.host, cert.port),
                ExchangePart::Trust,
                format!("{}:{}", cert.host, cert.port),
            );
            row.new = Some(cert.fingerprint.clone());
            let valid = plain_host(&cert.host)
                && cert.port > 0
                && certificate_fingerprint(&cert.fingerprint)
                && seen.insert(row.key.clone());
            if !valid {
                row.status = refused("exchange_file_invalid");
                rows.push(row);
                continue;
            }
            let there = stored
                .iter()
                .find(|stored| stored.host == cert.host && stored.port == cert.port);
            row.status = match there {
                None => RowStatus::New,
                Some(there) if there.fingerprint_sha256 == cert.fingerprint => RowStatus::Exists,
                Some(there) => {
                    row.old = Some(there.fingerprint_sha256.clone());
                    row.warning = Some("certificate_changed".to_owned());
                    RowStatus::Changed
                }
            };
            choose(&mut row, only);
            if !dry_run && row.selected {
                if let Some(there) = there {
                    db.remove_trusted_certificate(there.id).await?;
                }
                let new = NewTrustedCertificate {
                    host: cert.host.clone(),
                    port: cert.port,
                    fingerprint_sha256: cert.fingerprint.clone(),
                    subject: cert.subject.clone(),
                    issuer: cert.issuer.clone(),
                    not_after: cert.not_after,
                };
                row.status = match (db.add_trusted_certificate(&new).await?, there) {
                    (Some(_), None) => RowStatus::Created,
                    (Some(_), Some(_)) => RowStatus::Updated,
                    (None, _) => refused("exchange_file_invalid"),
                };
                changed = true;
            }
            rows.push(row);
        }
        if changed {
            self.publish(EngineEvent::TrustedCertificatesChanged);
        }
        Ok(rows)
    }
}
