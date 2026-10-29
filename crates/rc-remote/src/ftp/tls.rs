//! FTPS certificates: the system roots first, then a trust hook that has the last word.
//!
//! The verifier checks every handshake - the control connection and each data connection -
//! against the platform's roots (`rustls-platform-verifier`: CryptoAPI on Windows, the native
//! store elsewhere). Then the connection's [`CertificateTrust`] decides, with the certificate's
//! SHA-256 fingerprint and what the roots said ([`CertificateRejection::system_verified`]): a
//! certificate they do not vouch for (unknown CA, self-signed, wrong name, expired) passes only
//! when the hook trusts it, and the hook may also stop one they do vouch for (a server whose
//! stored certificate changed, RC-070-02). The default, [`RefuseUnknown`], follows the roots: the
//! connection fails with `certificate_unknown`, carrying the fingerprint a person has to compare.
//! The stored decisions are [`super::trust::PinnedCertificates`]; nothing here turns verification
//! off. The handshake signature is always checked, trusted or not.

use std::{
    fmt, io,
    sync::{Arc, Mutex, PoisonError},
};

use rustls::client::{ClientSessionMemoryCache, Resumption};
use rustls::{
    DigitallySignedStruct, Error as TlsError, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

use super::error::FtpError;

/// A certificate a server presented, as the trust hook sees it and as a refused connection
/// reports it. Public data only: what a person compares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CertificateRejection {
    pub host: String,
    pub port: u16,
    /// SHA-256 of the end-entity certificate, `AB:CD:...` in upper-case hex.
    pub fingerprint: String,
    /// Why the system roots refused it, as rustls words it; empty when they vouched for it.
    pub reason: String,
    /// Whether the system roots vouched for it. The hook still decides: a server whose stored
    /// certificate is another one is stopped all the same.
    pub system_verified: bool,
    /// The end-entity certificate (DER), for the subject, issuer and validity a dialog shows.
    pub end_entity: Vec<u8>,
}

/// What a [`CertificateTrust`] decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustDecision {
    Trust,
    Refuse,
}

/// Decides on every certificate a handshake presents, after the system roots. Called during the
/// handshake, so it answers from what it already knows; asking a person happens before the next
/// attempt.
pub trait CertificateTrust: Send + Sync {
    fn decide(&self, rejection: &CertificateRejection) -> TrustDecision;
}

/// Trusts what the system roots vouch for and refuses everything else: the default.
#[derive(Debug, Clone, Copy, Default)]
pub struct RefuseUnknown;

impl CertificateTrust for RefuseUnknown {
    fn decide(&self, rejection: &CertificateRejection) -> TrustDecision {
        if rejection.system_verified {
            TrustDecision::Trust
        } else {
            TrustDecision::Refuse
        }
    }
}

/// The SHA-256 fingerprint of a DER certificate, `AB:CD:...`.
#[must_use]
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The last refusal, read by the connection to tell `certificate_unknown` from other TLS errors
/// (suppaftp turns every handshake error into text).
type RejectionSlot = Arc<Mutex<Option<CertificateRejection>>>;

struct Verifier {
    /// The platform's roots; `None` when it has none at all: then no certificate is
    /// system-verified. A test puts a verifier over its own root here.
    system: Option<Arc<dyn ServerCertVerifier>>,
    provider: Arc<CryptoProvider>,
    trust: Arc<dyn CertificateTrust>,
    host: String,
    port: u16,
    rejected: RejectionSlot,
}

impl fmt::Debug for Verifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Verifier")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("system_roots", &self.system.is_some())
            .finish_non_exhaustive()
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let system = match &self.system {
            Some(system) => match system.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ) {
                Ok(_) => None,
                // Only a judgement on the certificate goes to the hook, never a broken handshake.
                Err(error @ TlsError::InvalidCertificate(_)) => Some(error),
                Err(other) => return Err(other),
            },
            None => Some(TlsError::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            )),
        };
        let rejection = CertificateRejection {
            host: self.host.clone(),
            port: self.port,
            fingerprint: fingerprint(end_entity.as_ref()),
            reason: system.as_ref().map(ToString::to_string).unwrap_or_default(),
            system_verified: system.is_none(),
            end_entity: end_entity.as_ref().to_vec(),
        };
        // What the handshake fails with when the hook refuses a certificate the roots accepted.
        let failure = system.unwrap_or(TlsError::InvalidCertificate(
            rustls::CertificateError::ApplicationVerificationFailure,
        ));
        match self.trust.decide(&rejection) {
            TrustDecision::Trust => Ok(ServerCertVerified::assertion()),
            TrustDecision::Refuse => {
                *self.rejected.lock().unwrap_or_else(PoisonError::into_inner) = Some(rejection);
                Err(failure)
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// The TLS side of one FTP server: a client configuration with the verifier, shared by every
/// connection to it, and per control connection a copy with a resumption store of its own
/// ([`TlsSetup::session`]). The control connection and its data connections ([`connect_data`])
/// use that copy and the configured host name, so
/// a data connection can only resume its own control connection's session: what servers that
/// require TLS session reuse demand (vsftpd's `require_ssl_reuse`, FileZilla Server).
pub(crate) struct TlsSetup {
    config: Arc<rustls::ClientConfig>,
    rejected: RejectionSlot,
}

/// TLS sessions one control connection keeps for its data connections to resume.
const SESSIONS_PER_CONTROL: usize = 8;

impl TlsSetup {
    pub fn new(host: &str, port: u16, trust: Arc<dyn CertificateTrust>) -> Result<Self, FtpError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let rejected = RejectionSlot::default();
        let verifier = Verifier {
            system: rustls_platform_verifier::Verifier::new(Arc::clone(&provider))
                .ok()
                .map(|system| Arc::new(system) as Arc<dyn ServerCertVerifier>),
            provider: Arc::clone(&provider),
            trust,
            host: host.to_owned(),
            port,
            rejected: Arc::clone(&rejected),
        };
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| FtpError::Tls(error.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        Ok(Self {
            config: Arc::new(config),
            rejected,
        })
    }

    /// The connector of one control connection and its data connections: the shared
    /// configuration with a fresh resumption store, so no other control connection's session is
    /// ever resumed.
    pub fn session(&self) -> TlsConnector {
        let mut config = rustls::ClientConfig::clone(&self.config);
        config.resumption = Resumption::store(Arc::new(ClientSessionMemoryCache::new(
            SESSIONS_PER_CONTROL,
        )));
        TlsConnector::from(Arc::new(config))
    }

    /// The error for a failed handshake: `certificate_unknown` when the verifier refused a
    /// certificate, the handshake's own text otherwise.
    pub fn handshake_error(&self, message: String) -> FtpError {
        let rejection = self
            .rejected
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match rejection {
            Some(rejection) => FtpError::CertificateUnknown(Box::new(rejection)),
            None => FtpError::Tls(message),
        }
    }
}

/// TLS on a data connection to `host`, with its control connection's connector
/// ([`TlsSetup::session`]): its verifier and its resumption store.
pub(crate) async fn connect_data(
    connector: &TlsConnector,
    host: &str,
    socket: TcpStream,
) -> io::Result<TlsStream<TcpStream>> {
    let name = ServerName::try_from(host.to_owned())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    connector.connect(name, socket).await
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;
