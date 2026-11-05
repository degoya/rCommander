//! The settings the gate reads for a call, in one query (RC-080-54): the policy, whether a
//! person allowed confirmation codes without a window, and the minutes an executing tool may be
//! allowed on its target, whether a path outside the panels asks, and whether the call is written
//! to the audit log (RC-080-66). Each decodes as it would alone: a policy that does not read refuses
//! the call (`policy_unavailable`), the two others fall back to their schema default with a
//! warning, as `rc_engine::settings::stored_setting` does.

use std::collections::BTreeMap;

use rc_db::{Db, DbError};
use rc_engine::settings::{
    MCP_ASK_OUTSIDE_PANELS_KEY, MCP_AUDIT_LOG_KEY, MCP_EXEC_SESSION_MINUTES_KEY,
    MCP_HEADLESS_CODE_KEY, setting,
};
use serde_json::Value;

use crate::policy::{POLICY_KEY, Policy};

/// What the gate reads for one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateSettings {
    pub(crate) policy: Policy,
    /// Confirmation codes stand in for the person when no window is open; no by default.
    pub(crate) headless_codes: bool,
    /// How long "allow for this session" holds for an executing tool's target; 0 offers none.
    pub(crate) exec_minutes: u32,
    /// A reading or writing tool asks when a path it names lies outside the panels' folders.
    pub(crate) ask_outside_panels: bool,
    /// The call is written to the audit log (`mcp_audit_log`); yes by default.
    pub(crate) audit_log: bool,
}

impl GateSettings {
    /// The settings, read at once.
    ///
    /// # Errors
    ///
    /// The read fails, or the stored policy is not a policy ([`DbError::Json`]).
    pub(crate) async fn load(db: &Db) -> rc_db::Result<Self> {
        let stored = db
            .get_json_many(&[
                POLICY_KEY,
                MCP_HEADLESS_CODE_KEY,
                MCP_EXEC_SESSION_MINUTES_KEY,
                MCP_ASK_OUTSIDE_PANELS_KEY,
                MCP_AUDIT_LOG_KEY,
            ])
            .await?;
        let policy: Policy = match stored.get(POLICY_KEY) {
            None => Policy::default(),
            Some(text) => serde_json::from_str(text).map_err(|source| DbError::Json {
                key: POLICY_KEY.to_owned(),
                source,
            })?,
        };
        let headless_codes = effective(&stored, MCP_HEADLESS_CODE_KEY)
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let exec_minutes = effective(&stored, MCP_EXEC_SESSION_MINUTES_KEY)
            .and_then(|value| value.as_u64())
            .and_then(|minutes| u32::try_from(minutes).ok())
            .unwrap_or(0);
        let ask_outside_panels = effective(&stored, MCP_ASK_OUTSIDE_PANELS_KEY)
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let audit_log = effective(&stored, MCP_AUDIT_LOG_KEY)
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        Ok(Self {
            policy,
            headless_codes,
            exec_minutes,
            ask_outside_panels,
            audit_log,
        })
    }
}

/// The value of the schema row `key` among `stored`: the stored one when it reads and fits, the
/// default otherwise; `None` for a key the schema does not have.
fn effective(stored: &BTreeMap<String, String>, key: &str) -> Option<Value> {
    let spec = setting(key)?;
    let Some(text) = stored.get(key) else {
        return Some(spec.default_value());
    };
    let checked = serde_json::from_str::<Value>(text)
        .map_err(|error| error.to_string())
        .and_then(|value| spec.check(&value));
    Some(checked.unwrap_or_else(|reason| {
        tracing::warn!(
            key,
            %reason,
            "a stored setting is not valid; its default applies"
        );
        spec.default_value()
    }))
}

#[cfg(test)]
#[path = "gate_settings_tests.rs"]
mod tests;
