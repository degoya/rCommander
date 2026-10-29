//! [`Engine::dispatch_mcp`]: the MCP server's only way into the engine, and
//! [`Engine::effective_class`], the class its gate asks by.

use rc_ops::JobSpec;

use crate::{
    Command, CommandClass, Engine, EngineError, Outcome, RetentionRequest,
    registry::{escalation, overwrites},
};

impl Engine {
    /// Runs one command for the MCP server: [`Engine::dispatch`], except that a
    /// [`CommandClass::GuiOnly`] command - a human decision about trust or a secret - is refused
    /// with `gui_only`, and so is a login-script change that touches a `send_secret` step
    /// (`Engine::refuse_mcp_secret_steps`); the command runs under the attached read judge
    /// (`read_guard.rs`), which also marks it as MCP's: an SSH session it opens never announces a
    /// locked vault (no unlock dialog for an agent's call: `vault_locked`). `rc-mcp` calls only
    /// this. A call the person allowed in the window runs with the panels it was asked about
    /// pinned (`mcp_pins.rs`, RC-080-34): one standing in another folder now refuses the command
    /// with `panel_changed`.
    ///
    /// # Errors
    ///
    /// [`EngineError::GuiOnly`], [`EngineError::SensitivePath`], [`EngineError::PanelChanged`],
    /// or the command's own failure.
    pub async fn dispatch_mcp(&self, command: Command) -> Result<Outcome, EngineError> {
        if command.spec().class == CommandClass::GuiOnly {
            return Err(EngineError::GuiOnly {
                command: command.id(),
            });
        }
        self.refuse_mcp_secret_steps(&command).await?;
        self.check_mcp_pins()?;
        let judge = self.inner.read_judge.get().copied();
        self.dispatch_judged(command, judge).await
    }

    /// The class an MCP call of `command` is gated as (RC-080-42): [`Command::effective_class`]
    /// from its parameters, and its escalation row's class where what is stored makes it so - a
    /// retention limit below the one in force (or limits that cannot be read), a saved queue
    /// list with a job that overwrites (or a list that cannot be read). `rc-mcp`'s gate asks by
    /// this and nothing else.
    pub async fn effective_class(&self, command: &Command) -> CommandClass {
        let class = command.effective_class();
        let stored = match command {
            Command::SetRetention(request) => self.lowers_retention(request).await,
            Command::QueueLoad { saved } => self.loads_overwriting(*saved).await,
            _ => false,
        };
        match escalation(command.id()) {
            Some(row) if stored => class.stricter(row.class),
            _ => class,
        }
    }

    /// Whether `request` sets a limit below the one in force: the next sweep deletes more.
    async fn lowers_retention(&self, request: &RetentionRequest) -> bool {
        if request.max_rows.is_none() && request.max_age_days.is_none() {
            return false;
        }
        let current = match self.db() {
            Ok(db) => db
                .retention_settings()
                .await
                .ok()
                .and_then(|settings| settings.limits(&request.table)),
            Err(_) => None,
        };
        current.is_none_or(|current| {
            request.max_rows.is_some_and(|rows| rows < current.max_rows)
                || request
                    .max_age_days
                    .is_some_and(|days| days < current.max_age_days)
        })
    }

    /// Whether the saved list `saved` holds a job that overwrites what it finds; an entry that
    /// does not read is never loaded. Without a database the load fails anyway.
    async fn loads_overwriting(&self, saved: i64) -> bool {
        let Ok(db) = self.db() else {
            return false;
        };
        match db.saved_queue(saved).await {
            Ok(Some((_, texts))) => texts.iter().any(|text| {
                serde_json::from_str::<JobSpec>(text)
                    .is_ok_and(|spec| overwrites(spec.options.on_conflict))
            }),
            Ok(None) => false,
            Err(_) => true,
        }
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
