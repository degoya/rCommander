//! What the MCP gate reads from what is stored for `usercmd.run` and `start_menu.run`
//! (RC-100-86, `Engine::effective_class`): a program that starts as administrator or asks for
//! its parameters (`?`), and an internal row stricter than `Exec`, make the call the person's
//! (`GuiOnly`) - also through own commands that call each other. A command that does not read or
//! is not there escalates nothing: the run refuses it anyway, and refuses what changed since the
//! gate judged (`run.rs`).

use std::{future::Future, pin::Pin};

use super::{CommandKind, CommandRef, MAX_DEPTH, OWN_PREFIX, StartItemKind, tc_names};
use crate::{Command, CommandClass, Engine, Keymap};

/// The boxed future of [`Engine::action_needs_person`]: own commands call each other.
pub(crate) type JudgeFuture<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

impl Engine {
    /// Whether what is stored makes an MCP call of `command` (`usercmd.run`, `start_menu.run`)
    /// a person's to make; `false` for any other command.
    pub(crate) async fn stored_run_needs_person(&self, command: &Command) -> bool {
        match command {
            Command::RunUserCommand { name, .. } => match self.user_command(name).await {
                Ok(found) => self.action_needs_person(&found.action, 0).await,
                Err(_) => false,
            },
            Command::RunStartItem { id, .. } => match self.start_item(*id).await {
                Ok(item) if item.kind == StartItemKind::Item => match &item.action {
                    Some(action) => self.action_needs_person(action, 0).await,
                    None => false,
                },
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `action`, at call depth `depth`, does something only a person may start.
    pub(crate) fn action_needs_person<'a>(
        &'a self,
        action: &'a CommandRef,
        depth: usize,
    ) -> JudgeFuture<'a> {
        Box::pin(async move {
            if depth > MAX_DEPTH {
                return false;
            }
            match action.kind {
                CommandKind::Program => action.elevated || action.asks(),
                CommandKind::Own => match self.user_command(&action.command).await {
                    Ok(target) => self.action_needs_person(&target.action, depth + 1).await,
                    Err(_) => false,
                },
                CommandKind::Internal => {
                    for row in action.rows() {
                        let lower = row.to_ascii_lowercase();
                        let needs = if lower.starts_with(OWN_PREFIX) {
                            match self.user_command(&lower).await {
                                Ok(target) => {
                                    self.action_needs_person(&target.action, depth + 1).await
                                }
                                Err(_) => false,
                            }
                        } else {
                            row_stricter_than_exec(&lower, row)
                        };
                        if needs {
                            return true;
                        }
                    }
                    false
                }
            }
        })
    }
}

/// Whether the keymap row `row` (or the Total Commander name `lower` maps to) runs a command
/// stricter than `Exec`; a window row or an unknown row is refused by the run itself.
fn row_stricter_than_exec(lower: &str, row: &str) -> bool {
    let name = if lower.starts_with("cm_") {
        match tc_names::row_of(lower) {
            Some(name) => name,
            None => return false,
        }
    } else {
        row
    };
    Keymap::shared_standard()
        .bindings()
        .iter()
        .find(|binding| binding.name == name)
        .is_some_and(|binding| {
            binding
                .command
                .effective_class()
                .stricter(CommandClass::Exec)
                != CommandClass::Exec
        })
}

#[cfg(test)]
#[path = "judge_tests.rs"]
mod tests;
