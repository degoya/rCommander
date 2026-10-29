//! At most [`MAX_LOG_LINES_PER_CALL`] lines a plugin writes per call.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/log_budget.rs` at commit
//! 26c8592a6 (RD-1260-12). There a store was one call; here a store lives for one opened document
//! (`docs/plugins.md`, "Long-lived stores"), so the budget starts again with every call
//! ([`LogBudget::begin`]) and the warning about the suppressed rest is written when the next call
//! begins or the store is dropped. Every line counts, whatever its level: a log flood must not
//! depend on the level the person's log runs at.

/// Lines one plugin call may write to the log.
pub const MAX_LOG_LINES_PER_CALL: u32 = 50;

/// Who a call's lines belong to: the plugin, its version and what was called.
#[derive(Clone, Debug, Default)]
pub(crate) struct LogOrigin {
    pub(crate) plugin_id: String,
    pub(crate) version: String,
    /// The export called, such as `open`.
    pub(crate) operation: &'static str,
}

/// One call's budget.
#[derive(Debug, Default)]
pub(crate) struct LogBudget {
    origin: LogOrigin,
    written: u32,
    suppressed: u64,
}

impl LogBudget {
    /// Names whose lines these are.
    pub(crate) fn new(plugin_id: String, version: String) -> Self {
        Self {
            origin: LogOrigin {
                plugin_id,
                version,
                operation: "",
            },
            written: 0,
            suppressed: 0,
        }
    }

    /// Starts the budget of the call `operation`, after reporting what the last one suppressed.
    pub(crate) fn begin(&mut self, operation: &'static str) {
        self.report();
        self.written = 0;
        self.suppressed = 0;
        self.origin.operation = operation;
    }

    /// Takes one line: `true` while the budget lasts.
    pub(crate) fn admit(&mut self) -> bool {
        if self.written < MAX_LOG_LINES_PER_CALL {
            self.written += 1;
            true
        } else {
            self.suppressed = self.suppressed.saturating_add(1);
            false
        }
    }

    pub(crate) fn origin(&self) -> &LogOrigin {
        &self.origin
    }

    #[cfg(test)]
    pub(crate) fn written(&self) -> u32 {
        self.written
    }

    #[cfg(test)]
    pub(crate) fn suppressed(&self) -> u64 {
        self.suppressed
    }

    /// One warning for the lines the call could not write.
    fn report(&mut self) {
        if self.suppressed == 0 {
            return;
        }
        tracing::warn!(
            code = "plugin_log_lines_suppressed",
            plugin_id = self.origin.plugin_id.as_str(),
            plugin_version = self.origin.version.as_str(),
            operation = self.origin.operation,
            suppressed = self.suppressed,
            "... {} more plugin log lines suppressed",
            self.suppressed
        );
        self.suppressed = 0;
    }
}

impl Drop for LogBudget {
    fn drop(&mut self) {
        self.report();
    }
}

#[cfg(test)]
#[path = "log_budget_tests.rs"]
mod tests;
