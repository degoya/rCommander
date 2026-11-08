//! A job command handed to the engine, and what became of it: its jobs, or the refusal the
//! panel's status line shows - a rename mask's (RC-110-04) with the names it is about.

use rc_engine::{Command, Engine, EngineError, JobSnapshot, Outcome, PanelId, RenameError};

use super::FollowUp;
use crate::i18n::t;

/// A job command for the engine, with what the window does with its job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submit {
    pub panel: PanelId,
    pub command: Command,
    pub follow: FollowUp,
    pub background: bool,
}

/// What became of a [`Submit`]: its job, or its jobs in their order - one per source
/// (RC-010-14).
#[derive(Debug, Clone)]
pub struct Submitted {
    pub panel: PanelId,
    pub follow: FollowUp,
    pub background: bool,
    pub result: Result<Vec<JobSnapshot>, &'static str>,
    /// The refusal's message with the names it is about, where the code's own text does not
    /// name them (a rename mask's collision or chain).
    pub detail: Option<String>,
    /// The command, when a locked vault refused it: the unlock prompt submits it again
    /// (RC-050-03).
    pub refused: Option<Command>,
}

impl Submit {
    /// Hands the command to `engine`; the job it accepted.
    pub async fn run(self, engine: Engine) -> Submitted {
        // Kept for a refusal; a command is a few words.
        let again = self.command.clone();
        let mut refused = None;
        let mut detail = None;
        let result = match engine.dispatch(self.command).await {
            Ok(Outcome::Job(job)) => Ok(vec![*job]),
            Ok(Outcome::Jobs { jobs }) if !jobs.is_empty() => Ok(jobs),
            // The ops commands answer with their jobs; anything else is no job to follow.
            Ok(_) => Err("io"),
            Err(error) => {
                if error.code() == "vault_locked" {
                    refused = Some(again);
                }
                detail = named(&error);
                Err(error.code())
            }
        };
        Submitted {
            panel: self.panel,
            follow: self.follow,
            background: self.background,
            result,
            detail,
            refused,
        }
    }
}

/// The message of a rename mask's refusal that names its entries.
pub(super) fn named(error: &EngineError) -> Option<String> {
    match error {
        EngineError::Rename(RenameError::MaskCollision {
            first,
            second,
            name,
        }) => Some(t!(
            "error.mask_collision",
            first = first,
            second = second,
            name = name
        )),
        EngineError::Rename(RenameError::MaskChain { entry, name }) => {
            Some(t!("error.mask_chain", entry = entry, name = name))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "submit_tests.rs"]
mod tests;
