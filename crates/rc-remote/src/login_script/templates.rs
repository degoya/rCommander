//! The templates a new script can start from. Their names are the window's: it shows each under
//! `login_scripts.template.<id>`.
//!
//! Each sends the connection's own password; the editor offers the script's stored secrets in
//! its place (`su - root` usually needs root's password, not the login's).

use super::{
    DEFAULT_STEP_TIMEOUT_MS, DEFAULT_TOTAL_TIMEOUT_MS, LoginScript, Pattern, SecretRef, Step,
};

/// A template: a stable id and the script it makes.
#[derive(Debug, Clone, Copy)]
pub struct Template {
    /// Stable; the i18n key of its name is `login_scripts.template.<id>`.
    pub id: &'static str,
    steps: fn() -> Vec<Step>,
}

impl Template {
    /// A new script from this template.
    #[must_use]
    pub fn script(&self) -> LoginScript {
        LoginScript {
            steps: (self.steps)(),
            step_timeout_ms: DEFAULT_STEP_TIMEOUT_MS,
            total_timeout_ms: DEFAULT_TOTAL_TIMEOUT_MS,
        }
    }
}

/// Every template, in the order the editor offers them.
pub const TEMPLATES: &[Template] = &[
    Template {
        id: "sudo_i",
        steps: sudo_i,
    },
    Template {
        id: "su_root",
        steps: su_root,
    },
    Template {
        id: "second_password",
        steps: second_password,
    },
];

/// The password prompt of `sudo`, `su` and most login programs.
const PASSWORD_PROMPT: &str = "(?i)password.*:";

/// A root shell's prompt at the end of the output.
const ROOT_PROMPT: &str = r"#\s*$";

/// Any common shell prompt at the end of the output.
const SHELL_PROMPT: &str = r"[$#>%]\s*$";

fn sudo_i() -> Vec<Step> {
    switch_user("sudo -i")
}

fn su_root() -> Vec<Step> {
    switch_user("su - root")
}

/// `command`, the password it asks for, then a root prompt.
fn switch_user(command: &str) -> Vec<Step> {
    vec![
        Step::SendText {
            text: command.to_owned(),
            newline: true,
        },
        wait(PASSWORD_PROMPT),
        password(),
        wait(ROOT_PROMPT),
    ]
}

/// A server that asks for a second password once the login is through.
fn second_password() -> Vec<Step> {
    vec![wait(PASSWORD_PROMPT), password(), wait(SHELL_PROMPT)]
}

fn wait(pattern: &str) -> Step {
    Step::WaitFor {
        pattern: Pattern::new(pattern).expect("a template's pattern compiles (templates_tests)"),
        timeout_ms: None,
    }
}

fn password() -> Step {
    Step::SendSecret {
        source: SecretRef::ConnectionPassword,
        newline: true,
    }
}

#[cfg(test)]
#[path = "templates_tests.rs"]
mod tests;
