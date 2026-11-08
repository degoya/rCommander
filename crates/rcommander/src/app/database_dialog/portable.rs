//! "Make this copy portable..." in the database dialog (RC-100-95): a question that says what
//! goes onto the drive, then `app.make_portable` - the checked copy of the database and the
//! installed plugins into `data` beside the program, the marker last - and the offer to restart,
//! since the running window keeps the folder it started with. Shown on the Backup tab, never on
//! macOS and not in a copy that is portable already (`crate::portable::offered`).

use iced::Element;
use rc_db::BackupReport;
use rc_engine::Command;

use super::{DatabaseDialog, DatabaseJob, DatabaseMsg, DialogMode, Phase};
use crate::{
    app::{App, Effect, Message, quit::QuitMsg},
    i18n::t,
    theme::{Tokens, fit::prose, layout},
};

/// Where the portable step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortableStep {
    #[default]
    Idle,
    /// The question is shown.
    Asking,
    /// `app.make_portable` runs.
    Copying,
    /// The copy is made; a restart takes it into use.
    Made,
}

/// The portable step's controls.
#[derive(Debug, Clone)]
pub enum PortableMsg {
    /// "Make this copy portable...": the question.
    Ask,
    Cancel,
    Confirm,
    /// "Restart now" after the copy.
    Restart,
}

fn msg(message: PortableMsg) -> Message {
    Message::Database(DatabaseMsg::Portable(message))
}

impl App {
    pub(super) fn on_portable(&mut self, message: PortableMsg) -> Vec<Effect> {
        let Some(dialog) = self.database.as_mut() else {
            return Vec::new();
        };
        match (message, dialog.portable) {
            (PortableMsg::Ask, PortableStep::Idle)
                if crate::portable::offered() && dialog.phase != Phase::Running =>
            {
                dialog.portable = PortableStep::Asking;
            }
            (PortableMsg::Cancel, PortableStep::Asking) => dialog.portable = PortableStep::Idle,
            (PortableMsg::Confirm, PortableStep::Asking) => {
                dialog.portable = PortableStep::Copying;
                dialog.phase = Phase::Running;
                self.info = Some(t!("db.portable_running").to_owned());
                return vec![Effect::Database(Box::new(DatabaseJob {
                    commands: vec![Command::MakePortable {}],
                    make_folder: None,
                }))];
            }
            (PortableMsg::Restart, PortableStep::Made) => {
                return self.on_quit(QuitMsg::Restart(true));
            }
            _ => {}
        }
        Vec::new()
    }

    /// Enter and Escape while the question or the restart offer shows; `None` otherwise.
    pub(super) fn portable_key(&mut self, key: &str) -> Option<Vec<Effect>> {
        let step = self.database.as_ref()?.portable;
        let message = match (step, key) {
            (PortableStep::Asking, "Enter") => PortableMsg::Confirm,
            (PortableStep::Asking, "Escape") => PortableMsg::Cancel,
            (PortableStep::Made, "Enter") => PortableMsg::Restart,
            (PortableStep::Copying, "Enter") => return Some(Vec::new()),
            _ => return None,
        };
        Some(self.on_portable(message))
    }
}

/// The engine answered while the copy ran: with its `report` the copy is made and this is the
/// text to show; without one it failed and the dialog is back where it was. `None` when the
/// answer was not the copy's.
pub(super) fn finished(
    dialog: &mut DatabaseDialog,
    report: Option<&BackupReport>,
) -> Option<String> {
    if dialog.portable != PortableStep::Copying {
        return None;
    }
    let Some(report) = report else {
        dialog.portable = PortableStep::Idle;
        return None;
    };
    dialog.portable = PortableStep::Made;
    let folder = report.path.parent().unwrap_or(&report.path).display();
    Some(t!("db.portable_done", folder = folder))
}

/// The button left in the button row of the Backup tab, while it is offered.
pub(super) fn offer<'a>(dialog: &DatabaseDialog) -> Vec<Element<'a, Message>> {
    let shown = dialog.mode == DialogMode::Backup
        && dialog.portable == PortableStep::Idle
        && crate::portable::offered();
    if !shown {
        return Vec::new();
    }
    let idle = dialog.phase != Phase::Running;
    vec![layout::secondary_button(
        t!("db.portable_offer"),
        idle.then(|| msg(PortableMsg::Ask)),
    )]
}

/// The question in place of the dialog's fields, and the restart offer after the copy; `None`
/// while neither shows.
pub(super) fn body<'a>(
    dialog: &'a DatabaseDialog,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let (text, primary, secondary) = match dialog.portable {
        PortableStep::Idle => return None,
        PortableStep::Asking => (
            prose(t!("db.portable_question")).color(tokens.text),
            layout::primary_button(t!("db.portable_confirm"), Some(msg(PortableMsg::Confirm))),
            layout::secondary_button(t!("db.portable_cancel"), Some(msg(PortableMsg::Cancel))),
        ),
        PortableStep::Copying => (
            prose(t!("db.portable_running")).color(tokens.text_secondary),
            layout::primary_button(t!("db.portable_confirm"), None),
            layout::secondary_button(t!("db.portable_cancel"), None),
        ),
        PortableStep::Made => {
            let done = match &dialog.phase {
                Phase::Done(text) => text.as_str(),
                _ => "",
            };
            (
                prose(done).color(tokens.accent),
                layout::primary_button(t!("db.portable_restart"), Some(msg(PortableMsg::Restart))),
                layout::secondary_button(
                    t!("db.close"),
                    Some(Message::Database(DatabaseMsg::Close)),
                ),
            )
        }
    };
    let buttons = layout::button_row(Some(primary), [secondary], Vec::new());
    Some(iced::widget::column![text, buttons].spacing(10).into())
}

#[cfg(test)]
#[path = "portable_tests.rs"]
mod tests;
