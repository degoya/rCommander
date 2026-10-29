//! A panel's file list: the column header that sorts, the rows in the full or the brief view,
//! the scroll bar (`scrollbar.rs`, dragged and clicked as in Windows, RC-010-23) and the status
//! line with its sums and the hidden-files switch. A panel on a server adds the rights column,
//! as `ls -l` prints them: always over SFTP, over FTP when the server lists them (RC-070-03). The
//! owner column follows where it is switched on and the backend names owners (RC-060-02). While a
//! long listing still arrives, the status line counts what came so far.
//!
//! Widgets are built only for [`Pane::visible_range`], one screen of rows whatever the
//! directory's length. The list area sits in a `sensor` that reports its size, from which
//! `update` learns how many rows and brief columns fit and how wide the name column is
//! (`columns.rs`, with the widths the person dragged, RC-010-20); the rows are drawn in `rows.rs`,
//! in the list's chosen font (RC-010-17). A change of its row height measures the list again, so
//! the paging counts the new rows.

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, mouse_area, row, sensor},
};
use rc_engine::PanelId;
use rc_vfs::{BackendId, Entry};

use super::{
    App, Message,
    context_menu::{self, ContextMessage},
    keys::PromptKind,
    panel::Pane,
};
use crate::{
    i18n::t,
    theme::{self, Tokens, fonts},
};

mod columns;
mod header;
mod rows;
mod scrollbar;

pub(super) use columns::{Column, ColumnWidths};
use columns::{brief_width, columns, split};
use header::column_header;
use rows::{Layout, RowState, brief_cell, cell, entry_row, parent_row};
use scrollbar::ScrollBar;

/// The kept masks shown beside the mask prompt; ArrowUp reaches the others.
const RECENT_MASKS: usize = 4;

impl App {
    /// The header (full view only) and the list of `side`.
    pub(super) fn file_list(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let pane = self.pane(side);
        if pane.brief() {
            self.list(side, tokens)
        } else {
            let header = column_header(pane, side, &self.resize.columns, tokens);
            column![header, self.list(side, tokens)].into()
        }
    }

    fn list(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let pane = self.pane(side);
        let active = side == self.active;
        let columns = shown_columns(pane, &self.resize.columns);
        // A slice, so the row closure stays `Copy` for the brief view's columns.
        let columns = columns.as_slice();
        let width = pane.width();
        let fonts = fonts::fonts();
        let font = fonts.list;
        let sizes = split(width, columns, &self.resize.columns, font.column_scale());
        let layout = Layout {
            columns,
            sizes: &sizes,
        };
        let line = move |row: usize| {
            if let Some(editor) = self.rename_editor(side, row, tokens) {
                return editor;
            }
            let state = RowState {
                cursor: row == pane.cursor_row(),
                active,
                selected: pane.is_selected(row),
                // By the row's place in the listing, so the stripe stays with its entry.
                alternate: theme::alternate(row),
                font,
                names: fonts.names,
            };
            let entry = pane.entry_index(row).map(|index| &pane.entries()[index]);
            let icon = self.icons.of(&pane.state.path, entry);
            let archive = pane.state.archive.as_deref();
            let line = match (pane.brief(), entry) {
                (false, Some(entry)) => {
                    let counted = pane.state.dir_sizes.get(&entry.name);
                    entry_row(entry, icon, archive, counted, state, layout, tokens)
                }
                (false, None) => parent_row(icon, state, layout, tokens),
                (true, entry) => {
                    let room = brief_width(width, pane.columns(), icon.is_some());
                    brief_cell(entry, icon, state, room, tokens)
                }
            };
            clickable(line, side, row)
        };
        let visible = pane.visible_range();
        let rows: Element<'_, Message> = if pane.brief() {
            let per_column = pane.page_rows();
            let columns = (0..pane.columns()).map(|index| {
                let start = visible.start + index * per_column;
                let end = (start + per_column).min(visible.end);
                column((start..end.max(start)).map(line))
                    .width(Length::FillPortion(1))
                    .into()
            });
            row(columns).spacing(4).into()
        } else {
            column(visible.map(line)).into()
        };
        let rows = container(container(rows).padding([2, 4]))
            .id(context_menu::list_id(side))
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true);
        let body = mouse_area(row![
            rows,
            ScrollBar::new(pane, side, tokens, Message::ScrollTo)
        ])
        .on_scroll(move |delta| Message::Scroll(side, delta))
        .on_right_press(Message::ContextMenu(ContextMessage::Beside(side)));
        // Keyed by the row height: a new one reports the size again.
        sensor(body)
            .on_show(move |size| Message::Viewport(side, size))
            .on_resize(move |size| Message::Viewport(side, size))
            .key(font.row_height().to_bits())
            .into()
    }

    /// The panel's status line, or the mask prompt while Num+ or Num- asks for one.
    pub(super) fn status_line(
        &self,
        side: PanelId,
        tokens: &'static Tokens,
    ) -> Element<'_, Message> {
        let pane = self.pane(side);
        let line: Element<'_, Message> = match &self.prompt {
            Some(prompt) if prompt.panel == side && prompt.kind != PromptKind::Path => {
                let label = if prompt.kind == PromptKind::Select {
                    t!("prompt.select")
                } else {
                    t!("prompt.deselect")
                };
                let field = super::path_bar::prompt_field(label, &prompt.text);
                row![field, self.recent_masks(tokens)]
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into()
            }
            _ => status_text(pane, tokens),
        };
        column![divider(tokens), container(line).padding([4, 12])].into()
    }
}

/// A row that a click puts the cursor on and a double-click opens.
fn clickable(line: Element<'_, Message>, side: PanelId, row: usize) -> Element<'_, Message> {
    mouse_area(line)
        .on_press(Message::Click(side, row))
        .on_double_click(Message::Open(side, row))
        .on_right_press(Message::ContextMenu(ContextMessage::Row(side, row)))
        .into()
}

impl App {
    /// The newest kept masks beside the mask prompt; a click takes one.
    fn recent_masks(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        let masks = self.mask_history.iter().take(RECENT_MASKS).map(|mask| {
            mouse_area(
                cell(mask.as_str(), theme::UI_FONT)
                    .size(theme::STATUS_SIZE)
                    .color(tokens.accent),
            )
            .on_press(Message::PromptRecall(mask.clone()))
            .into()
        });
        row(masks).spacing(10).into()
    }
}

fn status_text<'a>(pane: &'a Pane, tokens: &'static Tokens) -> Element<'a, Message> {
    let (message, color) = match &pane.error {
        Some(error) => (error.clone(), theme::DANGER),
        None if pane.state.listing.is_some() => {
            let count = crate::format::count(pane.state.entries.len() as u64);
            (t!("panel.reading", count = count), tokens.text_secondary)
        }
        None => {
            let sums = pane.summary();
            let text = t!(
                "panel.status",
                selected_size = crate::format::size(sums.selected_bytes),
                size = crate::format::size(sums.bytes),
                selected_files = crate::format::count(sums.selected_files as u64),
                files = crate::format::count(sums.files as u64),
                selected_dirs = crate::format::count(sums.selected_dirs as u64),
                dirs = crate::format::count(sums.dirs as u64),
            );
            (text, tokens.text_secondary)
        }
    };
    let hidden = if pane.state.show_hidden {
        t!("panel.hidden_shown")
    } else {
        t!("panel.hidden_off")
    };
    row![
        cell(message, theme::UI_FONT)
            .size(theme::STATUS_SIZE)
            .color(color)
            .width(Length::Fill),
        mouse_area(
            cell(hidden, theme::UI_FONT)
                .size(theme::STATUS_SIZE)
                .color(tokens.accent)
        )
        .on_press(Message::ToggleHidden),
    ]
    .spacing(8)
    .into()
}

/// The columns of `pane`'s full view, the rows' and the header's.
fn shown_columns(pane: &Pane, widths: &ColumnWidths) -> Vec<Column> {
    let owner = widths.owner && probe(pane, |entry| entry.owner.is_some());
    columns(pane.state.archive.is_some(), shows_rights(pane), owner)
}

/// Whether `pane`'s rows carry the rights column: on a server, whose rights matter there. An
/// FTP server that lists none (a Windows one, a DOS listing) gets no empty column.
fn shows_rights(pane: &Pane) -> bool {
    match pane.state.path.backend {
        BackendId::Sftp { .. } => true,
        BackendId::Ftp { .. } => probe(pane, |entry| entry.attributes.mode.is_some()),
        _ => false,
    }
}

/// Whether one of `pane`'s first [`PROBE`] entries has `what`: a column a backend fills or not
/// shows by them, so the answer costs the same in any folder.
fn probe(pane: &Pane, what: impl Fn(&Entry) -> bool) -> bool {
    pane.state.entries.iter().take(PROBE).any(what)
}

/// How many entries are looked at for a column the backend may not fill (rights, owner).
const PROBE: usize = 64;

pub(super) fn divider(tokens: &'static Tokens) -> Element<'static, Message> {
    container(Space::new().width(Length::Fill).height(1))
        .style(move |_| theme::divider(tokens))
        .into()
}
