//! Secrets a PAM system lends (RC-100-38): checked out at connect, given back when the session
//! ends.
//!
//! A source whose manifest says `[secrets] checkout = true` is asked `checkout` instead of `get`.
//! The lease is held here, in memory only, per connection and secret kind: another login of the
//! same connection while it lasts (an FTP control connection, an SSH session opened again, a login
//! script) takes the held value, so the system lends once per session. The lease goes back
//! (`checkin`) when the connection's SSH session ends - closed, lost or idle - unless another one
//! of it is up, at `connections.disconnect`, after an SSH test login or a failed SSH login, and for
//! every connection when rCommander ends ([`Engine::return_all_leases`]). A lease the system ends
//! by itself (`expires`, less [`EXPIRY_MARGIN_SECS`]) is not used again: the next connect checks
//! out anew. A failed checkin is logged by its code and dropped; the system takes the lease back at
//! its end anyway.
//!
//! When the system wants a reason (`checkout.reason_required`) and none is held for the connection,
//! the connect fails with `secret_source_reason_needed`; for the person's own call the engine
//! announces [`EngineEvent::CheckoutReasonNeeded`], the window asks for one and hands it over with
//! `secrets.checkout_reason` (`GuiOnly`), and the next connect passes it on. The reason is kept for
//! the connection until rCommander ends, or used up by one checkout while
//! `vault.checkout_keep_reason` is off. MCP never gives a reason: an agent's connect through a
//! connection the person mapped checks out as the person's does, and one that needs a reason fails
//! with the code.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rc_plugin_api::secrets::REASON_REQUIRED;
use rc_plugin_host::Lease;
use rc_vault::SecretString;

use super::{
    LoadedSource, SecretSourceError, SecretSourceRef, SecretSourcesOutcome, commands::answer,
};
use crate::recover::LockOrRecover as _;
use crate::{ConnectionId, Engine, EngineError, EngineEvent, Outcome, SecretKind};

/// The setting that keeps a connection's checkout reason for its later checkouts.
pub const CHECKOUT_KEEP_REASON_KEY: &str = "vault.checkout_keep_reason";

/// The longest reason `secrets.checkout_reason` takes: what the host hands to a plugin.
pub const MAX_CHECKOUT_REASON_CHARS: usize = rc_plugin_host::MAX_REASON_CHARS;

/// A lease that ends within this many seconds counts as ended: a login started with it might not
/// finish in time.
pub(crate) const EXPIRY_MARGIN_SECS: u64 = 30;

/// The leases held and the reasons given, by connection.
#[derive(Default)]
pub(crate) struct Leases {
    held: Mutex<HashMap<(ConnectionId, SecretKind), Held>>,
    reasons: Mutex<HashMap<ConnectionId, String>>,
}

/// One lease held for a connection's secret.
struct Held {
    source: String,
    entry_id: String,
    lease_id: String,
    value: SecretString,
    expires: Option<u64>,
}

impl Held {
    /// Whether the lease still stands for `reference` at `now`: the same source and entry, and
    /// not about to end.
    fn serves(&self, reference: &SecretSourceRef, now: u64) -> bool {
        self.source == reference.source
            && self.entry_id == reference.entry_id
            && self
                .expires
                .is_none_or(|end| now.saturating_add(EXPIRY_MARGIN_SECS) < end)
    }

    fn returned(self) -> Returned {
        Returned {
            source: self.source,
            lease_id: self.lease_id,
        }
    }
}

/// A lease taken out of the book, to be given back to its source.
pub(crate) struct Returned {
    source: String,
    lease_id: String,
}

/// What the book holds for one secret.
pub(crate) enum Found {
    /// A lease that still stands: its value.
    Value(SecretString),
    /// A lease that no longer stands, taken out to be given back.
    Stale(Returned),
    Nothing,
}

impl Leases {
    /// The held lease of `reference`'s connection and kind at `now`.
    pub(crate) fn find(&self, reference: &SecretSourceRef, now: u64) -> Found {
        let mut held = self.held.lock_or_recover("leases.held");
        let key = (reference.connection_id, reference.kind);
        match held.get(&key) {
            Some(lease) if lease.serves(reference, now) => Found::Value(lease.value.clone()),
            Some(_) => held
                .remove(&key)
                .map_or(Found::Nothing, |lease| Found::Stale(lease.returned())),
            None => Found::Nothing,
        }
    }

    /// Holds `lease` for `reference`; the one it replaces, lent meanwhile by a login beside it.
    fn hold(&self, reference: &SecretSourceRef, lease: Lease) -> Option<Returned> {
        let held = Held {
            source: reference.source.clone(),
            entry_id: reference.entry_id.clone(),
            lease_id: lease.id,
            value: lease.value,
            expires: lease.expires,
        };
        self.held
            .lock_or_recover("leases.held")
            .insert((reference.connection_id, reference.kind), held)
            .map(Held::returned)
    }

    /// Takes out every lease of `connection`.
    fn take(&self, connection: ConnectionId) -> Vec<Returned> {
        let mut held = self.held.lock_or_recover("leases.held");
        let keys: Vec<_> = held
            .keys()
            .filter(|(owner, _)| *owner == connection)
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|key| held.remove(&key))
            .map(Held::returned)
            .collect()
    }

    /// Takes out every lease.
    fn take_all(&self) -> Vec<Returned> {
        self.held
            .lock_or_recover("leases.held")
            .drain()
            .map(|(_, lease)| lease.returned())
            .collect()
    }

    /// How many leases `connection` holds.
    #[cfg(test)]
    pub(crate) fn held_by(&self, connection: ConnectionId) -> usize {
        self.held
            .lock_or_recover("leases.held")
            .keys()
            .filter(|(owner, _)| *owner == connection)
            .count()
    }

    fn reason(&self, connection: ConnectionId) -> Option<String> {
        self.reasons
            .lock_or_recover("leases.reasons")
            .get(&connection)
            .cloned()
    }

    fn set_reason(&self, connection: ConnectionId, reason: Option<String>) {
        let mut reasons = self.reasons.lock_or_recover("leases.reasons");
        match reason {
            Some(reason) => reasons.insert(connection, reason),
            None => reasons.remove(&connection),
        };
    }
}

/// Seconds since the Unix epoch; 0 for a clock before it.
pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// `reason` as `secrets.checkout_reason` keeps it: trimmed, `None` for nothing; refused when it is
/// too long or holds a control character.
pub(crate) fn checked_reason(reason: &str) -> Result<Option<String>, EngineError> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Ok(None);
    }
    if reason.chars().count() > MAX_CHECKOUT_REASON_CHARS || reason.chars().any(char::is_control) {
        return Err(SecretSourceError::Invalid {
            reason: "the checkout reason is too long or holds a control character",
        }
        .into());
    }
    Ok(Some(reason.to_owned()))
}

impl Engine {
    /// The lent secret `reference` points to, from `source`, which lends: checked out now, the
    /// person's reason passed on; held until the session ends.
    pub(crate) async fn checkout_secret(
        &self,
        reference: &SecretSourceRef,
        source: LoadedSource,
        announce: bool,
    ) -> Result<SecretString, EngineError> {
        let leases = &self.inner.remotes.leases;
        let connection = reference.connection_id;
        let reason = leases.reason(connection);
        match source
            .checkout(&reference.entry_id, reference.kind, reason.as_deref())
            .await
        {
            Ok(lease) => {
                self.remember_secret(&lease.value);
                let value = lease.value.clone();
                if reason.is_some() && !self.setting_on(CHECKOUT_KEEP_REASON_KEY).await {
                    leases.set_reason(connection, None);
                }
                if let Some(replaced) = leases.hold(reference, lease) {
                    self.give_back(replaced).await;
                }
                Ok(value)
            }
            Err(failure) if failure.code() == Some(REASON_REQUIRED) => {
                tracing::debug!(%connection, "a PAM system wants a reason for the checkout");
                // Never for an agent's call: no prompt opens for MCP (`read_guard::from_mcp`).
                if announce && !crate::read_guard::from_mcp() {
                    self.publish(EngineEvent::CheckoutReasonNeeded { connection });
                }
                Err(SecretSourceError::ReasonNeeded { name: source.name }.into())
            }
            Err(failure) => {
                let code = failure.code().unwrap_or("plugin_failed").to_owned();
                tracing::debug!(%connection, code, "a PAM system did not lend the secret");
                let state = source.status().await.ok();
                Err(SecretSourceError::from_state(source.name, state, failure.kind, code).into())
            }
        }
    }

    /// `secrets.checkout_reason`: the reason the next checkout of connection `id` hands over;
    /// an empty one forgets it.
    pub(crate) async fn set_checkout_reason(
        &self,
        id: ConnectionId,
        reason: &str,
    ) -> Result<Outcome, EngineError> {
        let reason = checked_reason(reason)?;
        // Only a stored connection maps a source; a quick one has nothing to lend for.
        self.db()?.connection(id).await?;
        let held = reason.is_some();
        self.inner.remotes.leases.set_reason(id, reason);
        Ok(answer(SecretSourcesOutcome::Reason {
            connection_id: id,
            held,
        }))
    }

    /// Gives back every lease of connection `id`.
    pub async fn return_leases(&self, id: ConnectionId) {
        for returned in self.inner.remotes.leases.take(id) {
            self.give_back(returned).await;
        }
    }

    /// Gives back the leases of connection `id` unless a session of it is up and may still need
    /// them: after a test login, a login that failed, a session that ended.
    pub(crate) async fn return_unused_leases(&self, id: ConnectionId) {
        if self.inner.sessions.slot(id).live().is_none() {
            self.return_leases(id).await;
        }
    }

    /// Gives back every lease held, as rCommander ends.
    pub async fn return_all_leases(&self) {
        for returned in self.inner.remotes.leases.take_all() {
            self.give_back(returned).await;
        }
    }

    /// One `checkin`; a failure is logged by its code and the lease forgotten.
    pub(crate) async fn give_back(&self, returned: Returned) {
        let source = match self.load_source(&returned.source).await {
            Ok(source) => source,
            Err(error) => {
                tracing::warn!(code = error.code(), "a lent secret could not be given back");
                return;
            }
        };
        if let Err(failure) = source.checkin(&returned.lease_id).await {
            tracing::warn!(
                code = failure.code().unwrap_or("plugin_failed"),
                "a lent secret could not be given back"
            );
        }
    }
}

#[cfg(test)]
#[path = "leases_tests.rs"]
mod tests;
