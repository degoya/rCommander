//! Where a rename job records each step it carried out.

use async_trait::async_trait;

/// Told of every step a [`crate::JobKind::Rename`] job carries out, before the job starts the
/// next one: a run cancelled, failed or broken off between two steps leaves no step - and no
/// temporary name - unrecorded. The engine's journal is the undo log in `rc-db`.
#[async_trait]
pub trait StepJournal: Send + Sync {
    /// Step `index` of the job's steps is carried out.
    ///
    /// # Errors
    ///
    /// Why the step cannot be recorded; the job stops as failed (`journal_failed`) before the
    /// next step.
    async fn done(&self, index: usize) -> Result<(), String>;

    /// Step `index`, carried out before, is reversed again: the job rolls a failed run back
    /// (RC-080-64). Nothing by default.
    ///
    /// # Errors
    ///
    /// Why the reversal cannot be recorded; the roll-back stops there.
    async fn undone(&self, _index: usize) -> Result<(), String> {
        Ok(())
    }
}
