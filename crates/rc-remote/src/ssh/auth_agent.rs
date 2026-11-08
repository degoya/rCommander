//! The login with what the SSH agents hold: their keys (RC-080-43) and the OpenSSH user
//! certificates among them (RC-060-09).
//!
//! An agent lists a key and its certificate as two identities of the same fingerprint. The
//! order and the limit count keys ([`offer_order`]); a key's turn offers its certificates first,
//! then the key itself - a server that trusts the CA lets the user in with the certificate, one
//! that knows the key with the key. A certificate outside its validity is passed over, and none
//! is offered with `use_certificates` off.

use russh::{
    client::Handle,
    keys::{Certificate, HashAlg, PublicKey, agent::AgentIdentity},
};
use tokio::time::timeout;

use super::{Attempt, cert::refused_for, rsa_hash};
use crate::{
    agent::{self, AGENT_WAIT, AgentConnection, AgentSourceKind},
    certificate,
    ssh::{AuthUsed, SessionOptions, SshError, agent_offer::offer_order, handler::ClientHandler},
};

/// What an agent holds that a login can offer.
enum Offer {
    Key(PublicKey),
    Certificate(Box<Certificate>),
}

impl Offer {
    /// The `SHA256:` fingerprint of the key, a certificate's certified key's.
    fn fingerprint(&self) -> String {
        match self {
            Self::Key(key) => key.fingerprint(HashAlg::Sha256).to_string(),
            Self::Certificate(certificate) => certificate
                .public_key()
                .fingerprint(HashAlg::Sha256)
                .to_string(),
        }
    }
}

/// One agent: its connection, its offers with their fingerprints, and the distinct
/// fingerprints among them in the agent's order.
struct Held {
    kind: AgentSourceKind,
    agent: AgentConnection,
    offers: Vec<(String, Offer)>,
    fingerprints: Vec<String>,
}

/// The keys and certificates of every agent: the connection's own key first, then the others in
/// the agents' order, at most [`SessionOptions::max_agent_keys`] keys; only the chosen one when a
/// fingerprint is set. An agent that does not answer is passed over.
pub(super) async fn by_agent(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    options: &SessionOptions,
) -> Result<Attempt, SshError> {
    let now = crate::totp::unix_now();
    let mut agents = Vec::new();
    for source in &options.agents {
        let mut agent = match agent::connect(source).await {
            Ok(agent) => agent,
            Err(error) => {
                tracing::debug!(%error, "SSH agent not used");
                continue;
            }
        };
        let identities = match timeout(AGENT_WAIT, agent.request_identities()).await {
            Ok(Ok(identities)) => identities,
            Ok(Err(error)) => {
                tracing::debug!(%error, agent = %source.kind(), "SSH agent did not list its keys");
                continue;
            }
            Err(_) => {
                tracing::debug!(agent = %source.kind(), "SSH agent did not answer in time");
                continue;
            }
        };
        let offers = offers(identities, options.use_certificates, now);
        let mut fingerprints: Vec<String> = Vec::new();
        for (fingerprint, _) in &offers {
            if !fingerprints.contains(fingerprint) {
                fingerprints.push(fingerprint.clone());
            }
        }
        agents.push(Held {
            kind: source.kind(),
            agent,
            offers,
            fingerprints,
        });
    }
    let listed: Vec<Vec<String>> = agents
        .iter()
        .map(|held| held.fingerprints.clone())
        .collect();
    let order = offer_order(
        &listed,
        options.agent_fingerprint.as_deref(),
        options.own_key.as_deref(),
        options.max_agent_keys,
    );
    let mut refusal = None;
    for &(index, at) in &order {
        let Some(Held {
            kind,
            agent,
            offers,
            fingerprints,
        }) = agents.get_mut(index)
        else {
            continue;
        };
        let Some(fingerprint) = fingerprints.get(at).cloned() else {
            continue;
        };
        let mut turn: Vec<&Offer> = offers
            .iter()
            .filter(|(held, _)| *held == fingerprint)
            .map(|(_, offer)| offer)
            .collect();
        // Certificates first; the sort keeps the agent's order otherwise.
        turn.sort_by_key(|offer| matches!(offer, Offer::Key(_)));
        for offer in turn {
            let accepted = |key_id: Option<String>| -> Result<Attempt, SshError> {
                Ok(Attempt::Accepted(AuthUsed::Agent {
                    source: *kind,
                    fingerprint: fingerprint.clone(),
                    certificate: key_id,
                }))
            };
            match offer {
                Offer::Key(key) => {
                    let hash = rsa_hash(handle, &key.algorithm()).await?;
                    match handle
                        .authenticate_publickey_with(user, key.clone(), hash, agent)
                        .await
                    {
                        Ok(result) if result.success() => return accepted(None),
                        Ok(_) => {}
                        Err(error) => tracing::debug!(%error, "the SSH agent did not sign"),
                    }
                }
                Offer::Certificate(cert) => {
                    let info = certificate::describe(cert, "");
                    let hash = rsa_hash(handle, &cert.algorithm()).await?;
                    match handle
                        .authenticate_certificate_with(user, (**cert).clone(), hash, agent)
                        .await
                    {
                        Ok(result) if result.success() => return accepted(Some(info.key_id)),
                        Ok(_) => {
                            refusal = refusal.or_else(|| refused_for(&info, user));
                        }
                        Err(error) => tracing::debug!(%error, "the SSH agent did not sign"),
                    }
                }
            }
            if handle.is_closed() {
                return Err(SshError::ConnectionLost);
            }
        }
    }
    Ok(if order.is_empty() {
        Attempt::Skipped
    } else {
        refusal.map_or(Attempt::Refused, Attempt::Certificate)
    })
}

/// What an agent's identities offer: its keys and, with `use_certificates`, its user
/// certificates valid at `now`, each with its fingerprint.
fn offers(
    identities: Vec<AgentIdentity>,
    use_certificates: bool,
    now: u64,
) -> Vec<(String, Offer)> {
    identities
        .into_iter()
        .filter_map(|identity| match identity {
            AgentIdentity::PublicKey { key, .. } => Some(Offer::Key(key)),
            AgentIdentity::Certificate {
                certificate: cert, ..
            } if use_certificates && cert.cert_type().is_user() => {
                let info = certificate::describe(&cert, "");
                match certificate::check_validity(&info, now) {
                    Ok(()) => Some(Offer::Certificate(Box::new(cert))),
                    Err(error) => {
                        tracing::debug!(code = error.code(), "an agent's certificate passed over");
                        None
                    }
                }
            }
            AgentIdentity::Certificate { .. } => None,
        })
        .map(|offer| (offer.fingerprint(), offer))
        .collect()
}
