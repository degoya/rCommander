//! The FTPS certificate trust store: the `tls_certificates` table (migration 0013, RC-070-02).
//!
//! Like `known_hosts.rs`, this layer stores and never decides: whether a presented certificate
//! is trusted, unknown, changed or expired is `rc-remote`'s verdict. One host and port keeps at
//! most one certificate; trusting ([`Db::add_trusted_certificate`]) never overwrites a stored
//! one, so a changed certificate goes in only after a person removed the old row
//! ([`Db::remove_trusted_certificate`]).

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{Db, Result};

/// The columns of a row, as a literal for `concat!` (sqlx 0.9 takes `&'static str` SQL).
macro_rules! columns {
    () => {
        "id, host, port, fingerprint_sha256, subject, issuer, not_after, added_at, last_seen_at"
    };
}

/// One trusted certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedCertificate {
    pub id: i64,
    /// The host name as the connection names it (lower case).
    pub host: String,
    pub port: u16,
    /// SHA-256 of the end-entity certificate (DER), `AB:CD:...`.
    pub fingerprint_sha256: String,
    /// The subject's distinguished name, as shown when it was trusted.
    pub subject: String,
    pub issuer: String,
    /// The end of the certificate's validity, Unix seconds.
    pub not_after: i64,
    /// When it was stored (UTC, ISO 8601).
    pub added_at: String,
    /// When a connection last presented it; `None` before the first one.
    pub last_seen_at: Option<String>,
}

/// A certificate to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrustedCertificate {
    pub host: String,
    pub port: u16,
    pub fingerprint_sha256: String,
    pub subject: String,
    pub issuer: String,
    pub not_after: i64,
}

#[derive(FromRow)]
struct Row {
    id: i64,
    host: String,
    port: u16,
    fingerprint_sha256: String,
    subject: String,
    issuer: String,
    not_after: i64,
    added_at: String,
    last_seen_at: Option<String>,
}

impl From<Row> for TrustedCertificate {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            host: row.host,
            port: row.port,
            fingerprint_sha256: row.fingerprint_sha256,
            subject: row.subject,
            issuer: row.issuer,
            not_after: row.not_after,
            added_at: row.added_at,
            last_seen_at: row.last_seen_at,
        }
    }
}

impl Db {
    /// Every trusted certificate, by host and port.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn trusted_certificates(&self) -> Result<Vec<TrustedCertificate>> {
        let rows: Vec<Row> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM tls_certificates ORDER BY host, port"
        ))
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(TrustedCertificate::from).collect())
    }

    /// The certificate trusted for `host` on `port`, if any. `host` is compared as given; the
    /// caller lower-cases it like the stored names.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn trusted_certificate_for(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Option<TrustedCertificate>> {
        let row: Option<Row> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM tls_certificates WHERE host = ? AND port = ?"
        ))
        .bind(host)
        .bind(i64::from(port))
        .fetch_optional(self.readers())
        .await?;
        Ok(row.map(TrustedCertificate::from))
    }

    /// Stores a certificate a person trusted. `None` when one is stored for the host and port
    /// already - this one or another: this call never replaces a certificate.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn add_trusted_certificate(
        &self,
        certificate: &NewTrustedCertificate,
    ) -> Result<Option<TrustedCertificate>> {
        let mut writer = self.writer().await;
        let row: Option<Row> = sqlx::query_as(concat!(
            "INSERT INTO tls_certificates (host, port, fingerprint_sha256, subject, issuer, ",
            "not_after) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (host, port) DO NOTHING RETURNING ",
            columns!()
        ))
        .bind(&certificate.host)
        .bind(i64::from(certificate.port))
        .bind(&certificate.fingerprint_sha256)
        .bind(&certificate.subject)
        .bind(&certificate.issuer)
        .bind(certificate.not_after)
        .fetch_optional(&mut *writer)
        .await?;
        Ok(row.map(TrustedCertificate::from))
    }

    /// Notes that a connection presented the certificate `id` just now.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn touch_trusted_certificate(&self, id: i64) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query(
            "UPDATE tls_certificates SET last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE id = ?",
        )
        .bind(id)
        .execute(&mut *writer)
        .await?;
        Ok(())
    }

    /// Removes the certificate `id`; answers the row it removed, `None` when there was none.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn remove_trusted_certificate(&self, id: i64) -> Result<Option<TrustedCertificate>> {
        let mut writer = self.writer().await;
        let row: Option<Row> = sqlx::query_as(concat!(
            "DELETE FROM tls_certificates WHERE id = ? RETURNING ",
            columns!()
        ))
        .bind(id)
        .fetch_optional(&mut *writer)
        .await?;
        Ok(row.map(TrustedCertificate::from))
    }
}

#[cfg(test)]
#[path = "tls_certificates_tests.rs"]
mod tests;
