//! A panel's file list: the column header that sorts, the rows in the full or the brief view or
//! the tiles of the thumbnails (`grid.rs`, RC-100-89), the scroll bar (`scrollbar.rs`, dragged
//! and clicked as in Windows, RC-010-23) and the status line with its sums and the hidden-files
//! switch. A panel on a server adds the rights column,
//! as `ls -l` prints them: always over SFTP, over FTP when the server lists them (RC-070-03). The
//! owner column follows where it is switched on and the backend names owners (RC-060-02). A column
//! set or the comments view (RC-100-90) shows its own columns instead (`shown.rs`). While a long
//! listing still arrives, the status line counts what came so far. In a git repository, with
//! `panels.git_status` on, the rows carry their git marks and the status line the branch
//! (`git_marks.rs`, RC-100-97).
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
use rc_engine::{ColumnSetSpec, ListMode, PanelId};
use rc_vfs::{BackendId, Entry};

use super::{
    App, Message,
    context_menu::{self, ContextMessage},
    keys::PromptKind,
    panel::Pane,
    resize::Resize,
};
use crate::{
    i18n::t,
    theme::{self, Tokens, fit, fonts},
};

mod columns;
mod git_marks;
mod grid;
mod header;
mod rows;
mod scrollbar;
mod shown;

pub(super) use columns::{Column, ColumnWidths};
use columns::{brief_width, columns, split_shown};
pub(super) use grid::GRID_CHROME;
use header::column_header;
use rows::{About, Layout, RowState, brief_cell, cell, entry_row, parent_row};
use scrollbar::ScrollBar;
use shown::Shown;

/// The kept masks shown beside the mask prompt; ArrowUp reaches the others.
const RECENT_MASKS: usize = 4;

impl App {
    /// The header (full view only) and the list of `side`.
    pub(super) fn file_list(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let pane = self.pane(side);
        if pane.thumbnails() {
            self.grid(side, tokens)
        } else if pane.brief() {
            self.list(side, tokens)
        } else {
            let columns = shown_columns(pane, &self.resize);
            let header = column_header(pane, side, &columns, tokens);
            column![header, self.list(side, tokens)].into()
        }
    }

    fn list(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let pane = self.pane(side);
        let active = side == self.active;
        let columns = shown_columns(pane, &self.resize);
        // A slice, so the row closure stays `Copy` for the brief view's columns.
        let columns = columns.as_slice();
        let width = pane.width();
        let fonts = fonts::fonts();
        let font = fonts.list;
        let sizes = split_shown(width, columns, font.column_scale());
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
            let git = pane.state.git.as_deref();
            let mark = entry.and_then(|entry| git.and_then(|git| git.mark(&entry.name)));
            let line = match (pane.brief(), entry) {
                (false, Some(entry)) => {
                    let about = About {
                        archive,
                        counted: pane.state.dir_sizes.get(&entry.name),
                        comment: pane.state.comment(&entry.name),
                        git: mark,
                    };
                    entry_row(entry, icon, about, state, layout, tokens)
                }
                (false, None) => parent_row(icon, state, layout, tokens),
                (true, entry) => {
                    let room = brief_width(width, pane.columns(), icon.is_some());
                    brief_cell(entry, icon, state, mark, room, tokens)
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
                let label = match prompt.kind {
                    PromptKind::Select => t!("prompt.select"),
                    PromptKind::Deselect => t!("prompt.deselect"),
                    // RC-100-92: Ctrl+F12 and the tab menu's "Rename tab".
                    PromptKind::FileFilter => t!("prompt.file_filter"),
                    PromptKind::TabTitle(_) | PromptKind::Path => t!("prompt.tab_title"),
                };
                let field = super::path_bar::prompt_field(label, &prompt.text);
                if !prompt.kind.takes_masks() {
                    return column![divider(tokens), container(field).padding([4, 12])].into();
                }
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
    // Cut at its end in the room the branch and the switch leave; they stay whole (RC-110-17).
    let message = fit::status_message(message, theme::STATUS_SIZE, color, tokens);
    let toggle = mouse_area(
        cell(hidden, theme::UI_FONT)
            .size(theme::STATUS_SIZE)
            .color(tokens.accent),
    )
    .on_press(Message::ToggleHidden);
    // The branch stands right, before the hidden-files switch (RC-100-97).
    let git = git_marks::status_part(pane.state.git.as_deref(), tokens);
    row![message].push(git).push(toggle).spacing(8).into()
}

/// The columns of `pane`'s full view, the rows' and the header's.
fn shown_columns(pane: &Pane, resize: &Resize) -> Vec<Shown> {
    match pane.state.mode {
        ListMode::Comments => return shown::custom(&ColumnSetSpec::comments(), None),
        ListMode::Custom { set } => {
            // A set not known (deleted, not read yet) shows the full view.
            if let Some(found) = resize.set(set) {
                return shown::custom(&found.spec, Some(set));
            }
        }
        ListMode::Full | ListMode::Brief | ListMode::Thumbnails => {}
    }
    let widths = &resize.columns;
    let owner = widths.owner && probe(pane, |entry| entry.owner.is_some());
    let columns = columns(pane.state.archive.is_some(), shows_rights(pane), owner);
    shown::full(&columns, widths)
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
