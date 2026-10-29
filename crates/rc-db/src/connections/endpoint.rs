//! Where a stored connection connects, kept while it holds a secret (RC-080-12, RC-080-50): an
//! MCP caller may not point a connection with a password, key, shared credential, secret
//! source or a login script that sends a secret at another host, port, kind or user - the next
//! connect would hand the secret to that endpoint. The check runs inside the change's own
//! `BEGIN IMMEDIATE`, so no secret stored between the look and the write slips past it.

use sqlx::SqliteConnection;

use crate::connections::{Connection, ConnectionError, ConnectionId, ConnectionPatch, TreeResult};

/// What a change of a stored connection may do to its endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndpointRule {
    /// Any change (the person in the window).
    #[default]
    Free,
    /// Not its host, port, kind or user while it holds a secret
    /// ([`ConnectionError::EndpointChangeNeedsGui`]): what an MCP caller may do.
    KeepWhileSecretsHeld,
}

/// Whether `patch` changes where `connection` connects: its host, port, kind or user.
#[must_use]
pub fn moves_endpoint(connection: &Connection, patch: &ConnectionPatch) -> bool {
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

/// Refuses `patch` of `current` under `rule`, read on the writer inside the change's
/// transaction.
///
/// # Errors
///
/// [`ConnectionError::EndpointChangeNeedsGui`], or the read fails.
pub(crate) async fn guard_endpoint(
    connection: &mut SqliteConnection,
    current: &Connection,
    patch: &ConnectionPatch,
    rule: EndpointRule,
) -> TreeResult<()> {
    if rule == EndpointRule::KeepWhileSecretsHeld
        && moves_endpoint(current, patch)
        && holds_secrets(connection, current.id).await?
    {
        return Err(ConnectionError::EndpointChangeNeedsGui { id: current.id });
    }
    Ok(())
}

/// Whether connection `id` holds a secret a new endpoint would receive: one stored with it, a
/// shared credential, a secret source, or a login script with a `send_secret` step - or steps
/// that are no JSON array, which this cannot read and so counts as sending one.
async fn holds_secrets(connection: &mut SqliteConnection, id: ConnectionId) -> TreeResult<bool> {
    let holds: Option<bool> = sqlx::query_scalar(
        "SELECT password_enc IS NOT NULL OR key_enc IS NOT NULL OR passphrase_enc IS NOT NULL \
         OR credential_id IS NOT NULL \
         OR EXISTS (SELECT 1 FROM secret_sources WHERE connection_id = c.id) \
         OR EXISTS (SELECT 1 FROM login_scripts AS s WHERE s.id = c.login_script_id \
             AND (json_type(s.steps) IS NOT 'array' \
                  OR EXISTS (SELECT 1 FROM json_each(s.steps) AS step \
                      WHERE json_extract(step.value, '$.step') = 'send_secret'))) \
         FROM connections AS c WHERE c.id = ?",
    )
    .bind(id.0)
    .fetch_optional(&mut *connection)
    .await?;
    holds.ok_or(ConnectionError::ConnectionNotFound { id })
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
