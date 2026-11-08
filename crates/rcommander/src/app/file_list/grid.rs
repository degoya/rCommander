//! The thumbnails' grid (RC-100-89): square tiles in lines, the picture's thumbnail - or the
//! entry's icon, large, while it loads and where there is none - over the name, shortened as the
//! lists shorten it with the whole name in a tooltip (`docs/design.md`). Only the lines in
//! [`Pane::visible_range`] are built. Cursor and marks colour the tile as they colour a row;
//! a click, a double click and the right button act as on a row. Ctrl and the wheel zoom
//! (`thumbnails.rs`).

use iced::{
    Alignment, ContentFit, Element, Length,
    widget::{
        Space, column, container,
        image::{FilterMethod, Image},
        mouse_area, row, sensor,
    },
};
use rc_engine::PanelId;

use super::{ScrollBar, clickable, columns::SCROLLBAR_WIDTH, rows::sized};
use crate::{
    app::{
        App, Message, context_menu, context_menu::ContextMessage, panel::Pane, thumbnails::TILE_PAD,
    },
    i18n::t,
    theme::{self, Tokens, fit, fonts},
};

/// The width the grid's tiles cannot use: the scroll bar and the list's padding.
pub const GRID_CHROME: f32 = SCROLLBAR_WIDTH + 2.0 * 4.0;

/// The largest an icon is drawn in a tile.
const MAX_ICON: f32 = 64.0;

impl App {
    /// The tiles of `side` on screen, in lines.
    pub(super) fn grid(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let pane = self.pane(side);
        let visible = pane.visible_range();
        let per_line = pane.columns().max(1);
        let lines = (visible.start..visible.end).step_by(per_line).map(|start| {
            let end = (start + per_line).min(visible.end);
            row((start..end).map(|at| self.tile(pane, side, at, tokens))).into()
        });
        let body = container(column(lines))
            .padding([2, 4])
            .id(context_menu::list_id(side))
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true);
        let body = mouse_area(row![
            body,
            ScrollBar::new(pane, side, tokens, Message::ScrollTo)
        ])
        .on_scroll(move |delta| Message::Scroll(side, delta))
        .on_right_press(Message::ContextMenu(ContextMessage::Beside(side)));
        // Keyed by the tile and the row height: either reports the size again.
        let key = (self.thumbnails.size(), fonts::list().row_height().to_bits());
        sensor(body)
            .on_show(move |size| Message::Viewport(side, size))
            .on_resize(move |size| Message::Viewport(side, size))
            .key(key)
            .into()
    }

    /// The tile of `at`, a row of `pane`: `..` or an entry.
    fn tile(
        &self,
        pane: &Pane,
        side: PanelId,
        at: usize,
        tokens: &'static Tokens,
    ) -> Element<'_, Message> {
        let fonts = fonts::fonts();
        let font = fonts.list;
        let cell = self.thumbnails.cell();
        let side_px = self.thumbnails.size() as f32;
        let dir = &pane.state.path;
        let entry = pane.entry_index(at).map(|index| &pane.entries()[index]);
        let picture: Element<'_, Message> =
            match entry.and_then(|entry| self.thumbnails.of(dir, entry)) {
                Some(tile) => Image::new(tile)
                    .width(side_px)
                    .height(side_px)
                    .content_fit(ContentFit::ScaleDown)
                    .filter_method(FilterMethod::Linear)
                    .into(),
                None => match self.icons.of(dir, entry) {
                    Some(icon) => {
                        let shown = (side_px / 2.0).clamp(16.0, MAX_ICON);
                        Image::new(icon)
                            .width(shown)
                            .height(shown)
                            .filter_method(FilterMethod::Linear)
                            .into()
                    }
                    None => Space::new().into(),
                },
            };
        let picture = container(picture)
            .width(side_px)
            .height(side_px)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        let cursor = at == pane.cursor_row();
        let selected = pane.is_selected(at);
        let label: Element<'_, Message> = match self.rename_editor(side, at, tokens) {
            Some(editor) => editor,
            None => {
                let full = entry.map_or_else(|| t!("panel.parent").to_owned(), |e| e.name.clone());
                let room = side_px;
                let shown = if fonts.names.cut_end {
                    font.shorten_end(&full, room)
                } else {
                    font.shorten(&full, room)
                };
                let shortened = shown != full.as_str();
                let rule = entry.and_then(|entry| {
                    theme::colour_rules::name_colour(&entry.name, entry.is_dir_like(), tokens.dark)
                });
                let text = sized(shown.into_owned(), font.font, font.size);
                let text = match (selected, rule) {
                    (true, _) => text.color(tokens.marked),
                    (false, Some(rule)) => text.color(rule),
                    (false, None) => text,
                };
                let text = container(text)
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                    .clip(true);
                if shortened {
                    fit::tip_after(text, full, fonts.names.tip_delay, tokens)
                } else {
                    text.into()
                }
            }
        };
        let active = side == self.active;
        let tile = container(column![picture, label].align_x(Alignment::Center))
            .width(cell.width)
            .height(cell.height)
            .padding(TILE_PAD)
            .clip(true)
            .style(move |_| theme::striped_row(tokens, cursor, active, selected, false));
        clickable(tile.into(), side, at)
    }
}
