//! [`Engine::dispatch_mcp`]: the MCP server's only way into the engine, and
//! [`Engine::effective_class`], the class its gate asks by.
//!
//! A class judged on what is stored holds only while it stays so (RC-080-66): the gate gets what
//! it judged with the class ([`Engine::judge_class`]) and runs the call with it held
//! ([`crate::with_mcp_judged`], `mcp_judged.rs`).

use rc_ops::JobSpec;

use crate::{
    Command, CommandClass, Engine, EngineError, McpJudged, Outcome, RetentionRequest,
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
    /// list with a job that overwrites (or a list that cannot be read), an own command, Start
    /// menu entry or button that starts as administrator, asks or runs a row stricter than
    /// `Exec`, and a file whose internal association's `open` does so (RC-100-91). `rc-mcp`'s
    /// gate asks by this and nothing else.
    pub async fn effective_class(&self, command: &Command) -> CommandClass {
        self.judge_class(command).await.0
    }

    /// [`Engine::effective_class`], and what it was judged on: the gate runs the call with it
    /// held ([`crate::with_mcp_judged`]), so stored content changed in between refuses the call.
    pub async fn judge_class(&self, command: &Command) -> (CommandClass, McpJudged) {
        let class = command.effective_class();
        let mut judged = McpJudged::default();
        let stored = match command {
            Command::SetRetention(request) => self.lowers_retention(request).await,
            Command::QueueLoad { saved } => {
                let (overwrites, entries) = self.loads_overwriting(*saved).await;
                judged.queue = entries.map(|entries| (*saved, entries));
                overwrites
            }
            // The run itself refuses what changed since (`user_commands/run.rs`).
            Command::RunUserCommand { .. } | Command::RunStartItem { .. } => {
                self.stored_run_needs_person(command).await
            }
            // The file's internal association, as Enter would run it (RC-100-91).
            Command::OpenFile { panel, path } => {
                self.open_needs_person(*panel, path.as_deref()).await
            }
            Command::RunButton { .. } => self.stored_button_needs_person(command).await,
            _ => false,
        };
        let class = match escalation(command.id()) {
            Some(row) if stored => class.stricter(row.class),
            _ => class,
        };
        (class, judged)
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

    /// Whether the saved list `saved` holds a job that overwrites what it finds, and the
    /// entries judged (`Some(None)` for a list that is not there; `None` where nothing was read);
    /// an entry that does not read is never loaded. Without a database the load fails anyway.
    async fn loads_overwriting(&self, saved: i64) -> (bool, Option<Option<Vec<String>>>) {
        let Ok(db) = self.db() else {
            return (false, None);
        };
        match db.saved_queue(saved).await {
            Ok(Some((_, texts))) => {
                let overwrites = texts.iter().any(|text| {
                    serde_json::from_str::<JobSpec>(text)
                        .is_ok_and(|spec| overwrites(spec.options.on_conflict))
                });
                (overwrites, Some(Some(texts)))
            }
            Ok(None) => (false, Some(None)),
            Err(_) => (true, None),
        }
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
