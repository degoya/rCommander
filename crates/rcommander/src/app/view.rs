//! Drawing: two panels side by side (or one panel's terminal over both, `terminal_maximize.rs`),
//! the command line (`cmdline/view.rs`), the function-key bar and the status bar. A panel is its
//! path bar (`path_bar.rs`), its file list or terminal, and its status line (`file_list.rs`).
//! The panels share their area as the divider between them says (`resize.rs`, RC-010-20).

use iced::{
    Element, Length,
    widget::{column, container, mouse_area, row, sensor, stack},
};
use rc_engine::PanelId;

use super::{
    App, Message, context_menu, database_dialog,
    dialog_windows::DialogWindow,
    drive_bar, multi_rename, palette, properties, quick_connect,
    resize::{self, ResizeMsg},
    search, status_bar, tabs,
};
use crate::theme::{self, Tokens};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let tokens = theme::tokens(self.dark);
        // The maximized panel takes the area of both; the other is hidden, not closed.
        let panels = match self.maximized() {
            Some(side) => row![self.panel_with_drives(side, Length::Fill, tokens)],
            None => {
                let left = self.resize.left_share();
                row![
                    self.panel_with_drives(PanelId::Left, Length::FillPortion(left), tokens),
                    resize::divider_grip(),
                    self.panel_with_drives(
                        PanelId::Right,
                        Length::FillPortion(1000 - left),
                        tokens
                    ),
                ]
            }
        }
        .height(Length::Fill);
        let area = |size| Message::Resize(ResizeMsg::Area(size));
        let panels = sensor(panels).on_show(area).on_resize(area);
        let bottom = column![
            self.command_line(tokens),
            self.file_ops_bottom(tokens),
            status_bar::status_bar(self, tokens)
        ]
        .spacing(4);
        // The menu bar rCommander draws where no system menu is attached (RC-080-33).
        let mut parts: Vec<Element<'_, Message>> = palette::menu_bar::view::bar(self, tokens)
            .into_iter()
            .collect();
        parts.extend(drive_bar::above_panels(self, tokens));
        parts.push(panels.into());
        parts.push(bottom.into());
        // macOS: the content reaches under the transparent title bar (`startup.rs`); its
        // height stays free for the traffic lights.
        let top = if cfg!(target_os = "macos") { 30.0 } else { 8.0 };
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
        // The tabs stand on the panel, no gap: the active one joins its list.
        parts.push(match tabs::tab_bar(self, side, tokens) {
            Some(bar) => column![bar, self.panel(side, tokens)].into(),
            None => self.panel(side, tokens),
        });
        column(parts)
            .spacing(4)
            .width(width)
            .height(Length::Fill)
            .into()
    }

    fn panel(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let active = side == self.active;
        let body = match self
            .quick_view_body(side, tokens)
            .or_else(|| self.terminal_body(side, tokens))
        {
            Some(terminal) => terminal,
            None => self.file_list(side, tokens),
        };
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
