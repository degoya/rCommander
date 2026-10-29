//! Enter and a double click on a checksum file (RC-020-10, owner 2026-10-19): a file whose
//! extension names an algorithm ([`Algorithm::of_file_name`]: `.sfv`, `.md5`, `.sha`, `.sha1`,
//! `.sha256`, `.sha512`, `.b3`, `.blake3`) is checked as Ctrl+Shift+F10 checks it - the same job,
//! the same result list - for exactly that file, whatever else is selected. With
//! `checksum.enter_verifies` off Enter and the double click open it as any other file. The context
//! menu offers "Verify checksums" for such files either way (`context_menu`).
//!
//! Total Commander hands such a file to its association on Enter; a check there is Files >
//! Verify Checksums, or an internal association with `cm_CRCcheck` a person sets up. rCommander
//! checks by default (the owner's wish), and the setting gives TC's behaviour back.

use iced::Task;
use rc_engine::{
    Algorithm, Command, Entry, PanelId,
    settings::{CHECKSUM_ENTER_KEY, stored_bool},
};

use super::{ChecksumMsg, verify_job};
use crate::app::{App, Effect, Message};

/// Whether a changed setting is `checksum.enter_verifies`.
pub fn concerns(key: &str) -> bool {
    key == CHECKSUM_ENTER_KEY
}

/// Whether `entry` is a file named as a checksum file.
pub(in crate::app) fn is_checksum_file(entry: &Entry) -> bool {
    !entry.is_dir_like() && Algorithm::of_file_name(&entry.name).is_some()
}

impl App {
    /// Enter's command for `panel`, past the `..` row: `file.open` on a checksum file under the
    /// cursor checks it. `None` leaves the command to the engine.
    pub(in crate::app) fn checksum_enter(
        &self,
        panel: PanelId,
        command: &Command,
    ) -> Option<Vec<Effect>> {
        let Command::OpenFile {
            panel: target,
            path: None,
        } = command
        else {
            return None;
        };
        let panel = target.unwrap_or(panel);
        let name = self.verified_on_enter(self.pane(panel).state.cursor_entry()?)?;
        Some(vec![verify_job(panel, name, false)])
    }

    /// A double click on `row` of `panel`: the click and the open ([`App::pointing`]), or on a
    /// checksum file the click and its check.
    pub(in crate::app) fn double_click(&mut self, panel: PanelId, row: usize) -> Vec<Effect> {
        let pane = self.pane(panel);
        let name = pane
            .entry_index(row)
            .and_then(|index| pane.entries().get(index))
            .and_then(|entry| self.verified_on_enter(entry));
        match name {
            Some(name) => vec![
                self.pointing(panel, row, false),
                verify_job(panel, name, false),
            ],
            None => vec![self.pointing(panel, row, true)],
        }
    }

    /// The name of `entry` when Enter checks it: a checksum file while the setting is on.
    fn verified_on_enter(&self, entry: &Entry) -> Option<String> {
        let check = !self.file_ops.enter_opens_checksums && is_checksum_file(entry);
        check.then(|| entry.name.clone())
    }

    /// Reads `checksum.enter_verifies` (the start, and after it changed).
    pub(in crate::app) fn checksum_enter_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { stored_bool(&db, CHECKSUM_ENTER_KEY).await },
            |on| Message::Checksum(ChecksumMsg::EnterVerifies(on)),
        )
    }
}

#[cfg(test)]
#[path = "enter_tests.rs"]
mod tests;
