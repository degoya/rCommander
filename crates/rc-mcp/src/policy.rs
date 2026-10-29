//! What MCP may do, per command class: the setting `mcp.policy`.
//!
//! Each class is `allowed`, `ask` (the person's answer first: in the window, or headless a
//! confirmation code where allowed, [`crate::ask`]) or `deny`. The defaults allow reading and
//! writing and ask before anything destructive or executing. Those two classes never run without
//! an answer: `allowed` for them reads as `ask`, so the setting can
//! tighten the rule but not drop it. `gui_only` commands are always denied: no setting opens
//! them (and `Engine::dispatch_mcp` refuses them besides). The settings dialog (RC-010-09)
//! edits the policy; `settings_set` cannot, as every `mcp.*` key is protected.

use rc_db::Db;
use rc_engine::CommandClass;
use serde::{Deserialize, Serialize};

/// The settings key.
pub const POLICY_KEY: &str = "mcp.policy";

/// The rule for one class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Runs.
    Allowed,
    /// Runs after the person's answer (`ask.rs`).
    Ask,
    /// Refused.
    Deny,
}

/// The rule of every class, as stored under [`POLICY_KEY`]; a missing class takes its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub read: Rule,
    pub write: Rule,
    pub destructive: Rule,
    pub exec: Rule,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            read: Rule::Allowed,
            write: Rule::Allowed,
            destructive: Rule::Ask,
            exec: Rule::Ask,
        }
    }
}

impl Policy {
    /// The stored policy, or the default when none is stored.
    ///
    /// # Errors
    ///
    /// The database read fails or the stored value is not a policy.
    pub async fn load(db: &Db) -> rc_db::Result<Self> {
        Ok(db.get(POLICY_KEY).await?.unwrap_or_default())
    }

    /// Stores the policy.
    ///
    /// # Errors
    ///
    /// The database write fails.
    pub async fn store(&self, db: &Db) -> rc_db::Result<()> {
        db.set(POLICY_KEY, self).await
    }

    /// The rule that applies to `class`.
    #[must_use]
    pub fn decide(&self, class: CommandClass) -> Rule {
        let rule = match class {
            CommandClass::Read => self.read,
            CommandClass::Write => self.write,
            CommandClass::Destructive => self.destructive,
            CommandClass::Exec => self.exec,
            CommandClass::GuiOnly => return Rule::Deny,
        };
        match (class, rule) {
            (CommandClass::Destructive | CommandClass::Exec, Rule::Allowed) => Rule::Ask,
            _ => rule,
        }
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
