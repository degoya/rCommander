//! The function-key bar at the bottom of the window, as in Total Commander: F3 to F8 with what
//! each does, read from the keymap - with Shift, Ctrl or Alt held, what the key does with it. A
//! click runs the key's command, as the key would.

use iced::{
    Element, Length,
    widget::{container, mouse_area, row},
};
use rc_engine::{Chord, Command, ExternalProgram, FileDialog, PanelView, SortKey};

use super::{FileOpsMsg, wrap};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{self, Tokens, fit::line},
};

/// The keys the bar shows.
const KEYS: [&str; 6] = ["F3", "F4", "F5", "F6", "F7", "F8"];

impl App {
    pub(super) fn fkey_bar(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        let held = self.modifiers_held();
        let buttons = KEYS.into_iter().map(|key| {
            let chord = Chord {
                key: key.to_owned(),
                ..held.clone()
            };
            let command = self.keymap.lookup(&chord, PanelView::Files).cloned();
            let label = command.as_ref().map_or("", label_of);
            let face = container(
                row![
                    line(key).color(tokens.text_secondary),
                    line(label).color(tokens.text)
                ]
                .spacing(6),
            )
            .padding([5, 12])
            .width(Length::Fill)
            .center_x(Length::Fill)
            .style(move |_| theme::key_button(tokens));
            match command {
                Some(command) if !label.is_empty() => mouse_area(face)
                    .on_press(wrap(FileOpsMsg::Key(Box::new(command))))
                    .into(),
                _ => face.into(),
            }
        });
        row(buttons).spacing(6).into()
    }
}

/// The bar's word for `command`; empty for a command it has none for.
pub fn label_of(command: &Command) -> &'static str {
    match command {
        Command::OpenExternal {
            program: ExternalProgram::Viewer,
            ..
        } => t!("fkey.view"),
        Command::OpenExternal {
            program: ExternalProgram::Editor,
            ..
        } => t!("fkey.edit"),
        Command::FileDialog { dialog, .. } => match dialog {
            FileDialog::Copy => t!("fkey.copy"),
            FileDialog::CopyHere => t!("fkey.copy_here"),
            FileDialog::Move => t!("fkey.move"),
            FileDialog::Rename => t!("fkey.rename"),
            FileDialog::Mkdir => t!("fkey.mkdir"),
            FileDialog::Delete => t!("fkey.delete"),
            FileDialog::DeletePermanent => t!("fkey.delete_permanent"),
            FileDialog::NewText => t!("fkey.new_text"),
        },
        Command::Sort { key, .. } => match key {
            SortKey::Name => t!("fkey.sort_name"),
            SortKey::Ext => t!("fkey.sort_ext"),
            SortKey::Date => t!("fkey.sort_date"),
            SortKey::Size => t!("fkey.sort_size"),
        },
        Command::SelectDifferences { .. } => t!("fkey.compare_dirs"),
        _ => "",
    }
}

#[cfg(test)]
#[path = "fkey_bar_tests.rs"]
mod tests;
