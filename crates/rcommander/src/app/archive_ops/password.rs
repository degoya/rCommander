//! The password an encrypted archive asks for while it unpacks: one prompt at a time, over no
//! other archive dialog (RC-080-44). A question that comes while a dialog is open waits in its
//! job - the queue keeps it - and the prompt opens for it once the dialog before it closes, the
//! oldest job first; none is replaced, so no job waits for an answer nobody can give. A job
//! takes its answer on its own thread and asks on until then, so the jobs answered are kept
//! (`FileOps::passwords_answered`) and not asked again until they took the answer or ask anew.

use rc_engine::{
    Command, EngineEvent, JobChange, JobId, JobSnapshot, Question, SecretParam, VfsPath,
};
use rc_vault::ExposeSecret as _;

use super::{ArchiveDialog, PasswordPrompt, Typed};
use crate::app::{App, Effect};

/// The prompt for `job`, which asks for the password of `archive`.
fn prompt(job: JobId, archive: &VfsPath, wrong: bool) -> ArchiveDialog {
    let archive = archive.path.file_name().map_or_else(
        || archive.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    ArchiveDialog::Password(PasswordPrompt {
        job,
        archive,
        wrong,
        typed: Typed::default(),
    })
}

impl App {
    /// An extraction asks for a password: the prompt opens unless an archive dialog is open; then
    /// the question waits for it to close ([`App::ask_waiting_password`]).
    pub(in crate::app) fn on_archive_event(&mut self, event: EngineEvent) -> Vec<Effect> {
        if let EngineEvent::Job {
            job,
            change:
                JobChange::Question {
                    question: Question::Password { archive, wrong },
                },
        } = event
        {
            // A question asked anew - the password did not fit - is asked again.
            self.file_ops
                .passwords_answered
                .retain(|answered| *answered != job);
            if self.archive.is_some() {
                tracing::debug!(job, "an archive password waits for the open dialog");
                return Vec::new();
            }
            self.archive = Some(prompt(job, &archive, wrong));
        }
        Vec::new()
    }

    /// No archive dialog is open any more: the prompt opens for the oldest job that still waits
    /// for a password and was not answered already.
    pub(super) fn ask_waiting_password(&mut self) {
        if self.archive.is_some() {
            return;
        }
        let jobs = self.engine.ops().jobs();
        let asking = |job: &JobSnapshot| matches!(job.question, Some(Question::Password { .. }));
        // A job that took its answer, or ended, is forgotten.
        let answered = &mut self.file_ops.passwords_answered;
        answered.retain(|id| jobs.iter().any(|job| job.id == *id && asking(job)));
        let answered = &self.file_ops.passwords_answered;
        let waiting = jobs.into_iter().find_map(|job| match job.question {
            Some(Question::Password { archive, wrong }) if !answered.contains(&job.id) => {
                Some(prompt(job.id, &archive, wrong))
            }
            _ => None,
        });
        self.archive = waiting;
    }

    /// The prompt's answer: the typed password, or none (Escape, or nothing typed) to skip.
    pub(super) fn answer_password(&mut self) -> Vec<Effect> {
        let Some(ArchiveDialog::Password(prompt)) = self.archive.take() else {
            return Vec::new();
        };
        let typed = SecretParam::from(prompt.typed.expose_secret());
        let password = (!typed.is_empty()).then_some(typed);
        self.file_ops.passwords_answered.push(prompt.job);
        let answer = Command::ArchivePassword {
            job: prompt.job,
            password,
        };
        vec![Effect::Dispatch(self.active, vec![answer])]
    }
}

#[cfg(test)]
#[path = "password_tests.rs"]
mod tests;
