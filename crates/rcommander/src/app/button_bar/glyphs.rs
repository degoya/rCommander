//! The icon of a button that runs rCommander's own commands and has no symbol file (RC-100-87):
//! a glyph of the app's icon font (`theme::icon`, Fluent UI System Icons) for its first row - a
//! keymap row, or Total Commander's `cm_*` name read through `tc_names` - as Total Commander's
//! bar shows an icon for each of its commands. A row without a glyph here keeps the button's
//! text; a program's or an own command's button keeps its symbol (an own command's own one,
//! `icon_path`), or on Windows the program's own icon (`symbols.rs`).
//!
//! A symbol of rCommander's own collection (RC-110-10, `rc-symbol:<name>` as a button's symbol or
//! an own command's `icon_path`) is a glyph too, of the collection's font ([`collection`]); a
//! name the collection does not have leaves the button its text.

use rc_engine::{
    BarButton, ButtonAction, CommandKind, UserCommand,
    user_commands::{symbols, tc_names},
};

use crate::{
    app::user_commands::known,
    theme::{icon::Icon, symbols::glyph_of},
};

/// The glyph of each keymap row a button bar knows, the default bar's first.
const ROWS: &[(&str, Icon)] = &[
    ("panel.refresh", Icon::Refresh),
    ("view_brief", Icon::BriefView),
    ("view_full", Icon::FullView),
    ("panel.invert_selection", Icon::InvertSelection),
    ("archive.pack", Icon::Pack),
    ("pack_and_delete", Icon::Pack),
    ("archive.extract", Icon::Unpack),
    ("files.find_dialog", Icon::FindFiles),
    ("panel.toggle_quick_view", Icon::QuickView),
    ("multi_rename.open_dialog", Icon::Rename),
    ("sync.open_dialog", Icon::Synchronize),
    ("panel.toggle_terminal", Icon::Terminal),
    ("terminal.open", Icon::Terminal),
    ("terminal.maximize", Icon::Terminal),
    ("copy", Icon::Copy),
    ("copy_here", Icon::Copy),
    ("move", Icon::Move),
    ("rename", Icon::Rename),
    ("mkdir", Icon::NewFolder),
    ("delete", Icon::Delete),
    ("delete_permanent", Icon::Delete),
    ("view_file", Icon::ViewFile),
    ("edit_file", Icon::EditFile),
    ("new_text_file", Icon::EditFile),
    ("file.open", Icon::Open),
    ("compare.by_content", Icon::Compare),
    ("panel.swap", Icon::Swap),
    ("swap_with_tabs", Icon::Swap),
    ("parent_folder", Icon::ParentFolder),
    ("history.list", Icon::History),
    ("hotlist.list", Icon::Star),
    ("panel.select_all", Icon::SelectAll),
    ("archive.test", Icon::Test),
    ("connections.manager", Icon::Connect),
    ("connections.quick_connect", Icon::Connect),
    ("queue.window", Icon::Queue),
    ("app.quit", Icon::Quit),
];

/// The glyph of the keymap row `row` (or the Total Commander name it stands for); `None` for a
/// row without one.
pub fn of_row(row: &str) -> Option<Icon> {
    let row = tc_names::row_of(row).unwrap_or(row);
    ROWS.iter()
        .find(|(name, _)| *name == row)
        .map(|(_, icon)| *icon)
}

/// The glyph `button` shows without a symbol file: its first internal row's; `None` for a
/// program, an own command, a sub-bar or a row without one.
pub fn of(button: &BarButton) -> Option<Icon> {
    if button.icon.is_some() {
        return None;
    }
    match &button.action {
        ButtonAction::Command(action) if action.kind == CommandKind::Internal => {
            action.rows().first().copied().and_then(of_row)
        }
        _ => None,
    }
}

/// The glyph of the symbol collection `button` shows: its own symbol's when it names one, else -
/// for an own command's button without a symbol - the own command's; `None` for a file, nothing,
/// or a name the collection does not have.
pub fn collection(button: &BarButton) -> Option<char> {
    // The own commands are copied only for a button that needs them.
    let own = button.icon.is_none()
        && matches!(&button.action, ButtonAction::Command(action) if action.kind == CommandKind::Own);
    let commands = if own { known::commands() } else { Vec::new() };
    collection_among(button, &commands)
}

/// [`collection`] with the own commands `commands`.
pub fn collection_among(button: &BarButton, commands: &[UserCommand]) -> Option<char> {
    if let Some(icon) = &button.icon {
        return glyph_of(&icon.path);
    }
    match &button.action {
        ButtonAction::Command(action) if action.kind == CommandKind::Own => commands
            .iter()
            .find(|command| command.name.eq_ignore_ascii_case(action.command.trim()))
            .and_then(|command| glyph_of(&command.icon_path)),
        _ => None,
    }
}

/// Whether `icon_path` names a symbol of the collection - known or not - rather than a file to
/// read.
pub fn is_collection(icon_path: &str) -> bool {
    symbols::is_reference(icon_path)
}

#[cfg(test)]
#[path = "glyphs_tests.rs"]
mod tests;
