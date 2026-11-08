//! Drawing: two panels side by side (or one panel's terminal over both, `terminal_maximize.rs`),
//! the command line (`cmdline/view.rs`), the function-key bar and the status bar. A panel is its
//! path bar (`path_bar.rs`), its file list, terminal or folder tree, and its status line
//! (`file_list.rs`); a separate folder tree stands left of each panel's body or left of both
//! panels (`tree/view.rs`, RC-100-88).
//! The panels share their area as the divider between them says (`resize.rs`, RC-010-20). The
//! button bar stands under the menu bar, the vertical one beside the divider; the drive bar, the
//! command line and the status bar each show while their setting says so (`button_bar.rs`,
//! RC-100-87).
//! A panel on a server, or showing an SSH terminal, draws its tabs and its card in its
//! connection's theme (RC-100-51, [`App::panel_tokens`]); the drive bar, the bottom and every
//! dialog stay in the window's.

use iced::{
    Element, Length,
    widget::{column, container, mouse_area, row, sensor, stack},
};
use rc_engine::PanelId;

use super::{
    App, Message, button_bar, context_menu, database_dialog,
    dialog_windows::DialogWindow,
    drive_bar, multi_rename, palette, properties, quick_connect,
    resize::{self, ResizeMsg},
    search, status_bar, tabs,
    tools::{disk_usage_view, select_duplicates_view},
    tree,
};
use crate::theme::{self, Tokens};

impl App {
    /// The main window's title, fixed as Total Commander's (owner, 2026-10-18): name, version
    /// and author; the panels show the path. A dialog's window has the dialog's.
    pub(in crate::app) fn title(&self, id: iced::window::Id) -> String {
        self.window_title(id)
    }

    /// The look of every window: the window's theme of the mode - an active workspace's first
    /// (RC-100-68, `theme::themes`).
    pub(in crate::app) fn theme(&self, _window: iced::window::Id) -> Option<iced::Theme> {
        Some(theme::iced_theme(theme::tokens(self.dark)))
    }

    pub fn view(&self) -> Element<'_, Message> {
        let tokens = theme::tokens(self.dark);
        // The maximized panel takes the area of both; the other is hidden, not closed.
        let panels = match self.maximized() {
            Some(side) => row![self.panel_with_drives(side, Length::Fill, tokens)],
            None => {
                let left = self.resize.left_share();
                let mut line = row![
                    self.panel_with_drives(PanelId::Left, Length::FillPortion(left), tokens),
                    resize::divider_grip(),
                ];
                if let Some(bar) = button_bar::vertical(self, tokens) {
                    line = line.push(bar);
                }
                line.push(self.panel_with_drives(
                    PanelId::Right,
                    Length::FillPortion(1000 - left),
                    tokens,
                ))
            }
        }
        .height(Length::Fill);
        let area = |size| Message::Resize(ResizeMsg::Area(size));
        let panels: Element<'_, Message> = sensor(panels).on_show(area).on_resize(area).into();
        // One separate tree for both panels stands left of them, outside their measured area.
        let panels = match tree::separate_one(self, tokens) {
            Some(beside) => row![beside, panels].height(Length::Fill).into(),
            None => panels,
        };
        let shows = self.button_bar.settings;
        let mut bottom = column![].spacing(4);
        if shows.command_line {
            bottom = bottom.push(self.command_line(tokens));
        }
        bottom = bottom.push(self.file_ops_bottom(tokens));
        if shows.status_bar {
            bottom = bottom.push(status_bar::status_bar(self, tokens));
        }
        // The menu bar rCommander draws where no system menu is attached (RC-080-33).
        let mut parts: Vec<Element<'_, Message>> = palette::menu_bar::view::bar(self, tokens)
            .into_iter()
            .collect();
        parts.extend(button_bar::horizontal(self, tokens));
        parts.extend(drive_bar::above_panels(self, tokens));
        parts.push(panels);
        parts.push(bottom.into());
        // macOS with the Mac's look: the content reaches under the transparent title bar
        // (`startup.rs`); its height stays free for the traffic lights.
        let top = if theme::mac_look() { 30.0 } else { 8.0 };
        let window = container(column(parts).spacing(8))
            .padding(iced::Padding {
                top,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| theme::window(tokens));
        let window = drive_bar::with_menu(self, window.into(), tokens);
        let window = tabs::with_tabs_overlay(self, window, tokens);
        let window = context_menu::with_menu(self, window, tokens);
        let window = button_bar::with_menu(self, window, tokens);
        let window = palette::menu_bar::view::with_menu(self, window, tokens);
        let window = self.queue_layer(window, tokens);
        let window = self.file_ops_layer(window, tokens);
        let window = self.drag_layer(window, tokens);
        // In windows of their own (RC-080-15), these large dialogs draw there.
        let search = self.search.as_ref().filter(|dialog| dialog.open);
        let window = match search.filter(|_| !self.windowed(DialogWindow::Search)) {
            Some(dialog) => stack![window, search::overlay(dialog, tokens)].into(),
            None => window,
        };
        let rename = self.multi_rename.as_ref().filter(|dialog| dialog.open);
        let window = match rename.filter(|_| !self.windowed(DialogWindow::MultiRename)) {
            Some(dialog) => stack![window, multi_rename::overlay(dialog, tokens)].into(),
            None => window,
        };
        let window = self.with_lister(window, tokens);
        let window = self.compare_layer(window, tokens);
        // So do the standalone dialogs (RC-080-23), and the archive's password as a question.
        let archive = self.archive.as_ref().filter(|_| !self.dialog_windows.own);
        let window = match archive.filter(|_| !self.windowed(DialogWindow::Archive)) {
            Some(dialog) => stack![window, super::archive_ops::overlay(dialog, tokens)].into(),
            None => window,
        };
        let checksum = self.checksum.as_ref().filter(|_| {
            !self.windowed(DialogWindow::Checksum) && !self.windowed(DialogWindow::ChecksumForm)
        });
        let window = match checksum {
            Some(open) => stack![window, super::checksum::overlay(open, tokens)].into(),
            None => window,
        };
        let database = self.database.as_ref();
        let database = database.filter(|_| !self.windowed(DialogWindow::Database));
        let props = self.properties.as_ref();
        let props = props.filter(|_| !self.windowed(DialogWindow::Properties));
        let dialog = match (database, props) {
            (Some(dialog), _) => Some(database_dialog::overlay(dialog, tokens)),
            (None, Some(dialog)) => Some(properties::overlay(dialog, tokens)),
            (None, None) => None,
        };
        let window = match dialog {
            Some(dialog) => stack![window, dialog].into(),
            None => window,
        };
        let quick = self.quick.as_ref();
        let window = match quick.filter(|_| !self.windowed(DialogWindow::QuickConnect)) {
            Some(dialog) => stack![window, quick_connect::overlay(dialog, tokens)].into(),
            None => window,
        };
        // The tree dialog (Alt+F10, RC-100-88), as a card while it has no window of its own.
        let tree_dialog = self.tree.dialog.as_ref();
        let window = match tree_dialog.filter(|_| !self.windowed(DialogWindow::Tree)) {
            Some(dialog) => stack![window, tree::dialog_overlay(dialog, tokens)].into(),
            None => window,
        };
        // "Select duplicate files" and the disk-usage window (RC-100-98) as cards.
        let select = self.tools.select.as_ref().filter(|dialog| dialog.open);
        let window = match select.filter(|_| !self.windowed(DialogWindow::SelectDuplicates)) {
            Some(dialog) => stack![window, select_duplicates_view::overlay(dialog, tokens)].into(),
            None => window,
        };
        let usage = self.tools.usage.as_ref();
        let window = match usage.filter(|_| !self.windowed(DialogWindow::DiskUsage)) {
            Some(usage) => stack![window, disk_usage_view::overlay(usage, tokens)].into(),
            None => window,
        };
        match &self.palette {
            Some(palette) => stack![window, super::palette::overlay(palette, tokens)].into(),
            None => window,
        }
    }

    /// A panel under its drive bar (when each panel has one) and its drive line, `width` wide.
    fn panel_with_drives(
        &self,
        side: PanelId,
        width: Length,
        tokens: &'static Tokens,
    ) -> Element<'_, Message> {
        let mut parts = drive_bar::above_panel(self, side, tokens);
        let own = self.panel_tokens(side);
        // The tabs stand on the panel, no gap: the active one joins its list.
        parts.push(match tabs::tab_bar(self, side, own) {
            Some(bar) => column![bar, self.panel(side, own)].into(),
            None => self.panel(side, own),
        });
        column(parts)
            .spacing(4)
            .width(width)
            .height(Length::Fill)
            .into()
    }

    /// The look of `side`'s panel: the theme of the connection or the WSL distribution whose
    /// terminal it shows, else of the server or the distribution's share it stands on; the
    /// window's on this machine (RC-100-51, RC-100-56).
    pub(super) fn panel_tokens(&self, side: PanelId) -> &'static Tokens {
        if let Some(terminal) = self.shown_terminal(side) {
            if let Some(connection) = self.engine.terminal_connection(terminal) {
                return theme::themes::of_connection(Some(connection.0), self.dark);
            }
            if let Some(distro) = self.engine.terminal_distro(terminal) {
                return theme::themes::of_distro(Some(&distro), self.dark);
            }
        }
        theme::themes::of_path(&self.pane(side).state.path, self.dark)
    }

    fn panel(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let active = side == self.active;
        let body = match self
            .quick_view_body(side, tokens)
            .or_else(|| self.terminal_body(side, tokens))
            .or_else(|| tree::panel_body(self, side, tokens))
        {
            Some(terminal) => terminal,
            None => self.file_list(side, tokens),
        };
        let body = tree::separate_beside(self, side, body, tokens);
        let card = container(column![
            self.path_bar(side, tokens),
            body,
            self.status_line(side, tokens),
        ])
        // Inside the border: the path bar's own background covered the accent border above.
        .padding(1)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| theme::panel(tokens, active));
        // A click inside the terminal never reaches a `mouse_area` (the terminal captures it), so
        // the panel under the pointer is tracked by hover and activated on any mouse press.
        mouse_area(card)
            .on_enter(Message::Hover(side))
            .on_exit(Message::Unhover(side))
            .into()
    }
}
