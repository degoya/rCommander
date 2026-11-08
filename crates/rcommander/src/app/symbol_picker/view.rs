//! Drawing the picker (RC-110-10) as a page of the window that opened it: its title, the search,
//! the shelves on the left and the grid of symbols at the bars' size on the right, the keys'
//! help line, then the one button row - the preview at its left (the symbol large and as the
//! button shows it, its name, the button's tooltip), OK and Cancel. Each symbol is a glyph of the
//! collection's font in the theme's text colour (`theme::symbols`).

use iced::{
    Alignment, Element, Length,
    widget::{Column, Row, Space, column, container, mouse_area, row, scrollable, text},
};
use rc_engine::user_commands::symbols::{self, Category};

use super::{
    COLUMNS, GRID_SCROLL_ID, GRID_STOP_ID, PickFocus, PickerMsg, SEARCH_FIELD, SHELVES_STOP_ID,
    Shelf, SymbolPicker,
};
use crate::{
    app::{Message, drive_bar::button_style, user_commands::field},
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The shelves' width.
const SHELVES_WIDTH: f32 = 200.0;
/// The large symbol of the preview.
const PREVIEW_SIZE: f32 = 48.0;
/// The room round a symbol in its cell of the grid.
const CELL_PADDING: f32 = 16.0;

/// The words of `shelf` in the list.
pub fn shelf_name(shelf: Shelf) -> &'static str {
    match shelf {
        Shelf::Recent => t!("symbol_picker.shelf.recent"),
        Shelf::All => t!("symbol_picker.shelf.all"),
        Shelf::Category(category) => category_name(category),
    }
}

/// The words of `category`, translated.
pub fn category_name(category: Category) -> &'static str {
    match category {
        Category::Files => t!("symbol_picker.category.files"),
        Category::Edit => t!("symbol_picker.category.edit"),
        Category::Tools => t!("symbol_picker.category.tools"),
        Category::Network => t!("symbol_picker.category.network"),
        Category::Development => t!("symbol_picker.category.development"),
        Category::Media => t!("symbol_picker.category.media"),
        Category::System => t!("symbol_picker.category.system"),
        Category::Arrows => t!("symbol_picker.category.arrows"),
        Category::Status => t!("symbol_picker.category.status"),
    }
}

/// `symbol` of the collection's font at `size`, in the text colour.
fn glyph<'a>(symbol: char, size: f32) -> Element<'a, Message> {
    text(symbol)
        .font(theme::symbols::FONT)
        .size(size)
        .line_height(text::LineHeight::Relative(1.0))
        .center()
        .width(size)
        .height(size)
        .into()
}

/// The page: `map` makes the window's message of a picker's.
pub fn page<'a>(
    picker: &'a SymbolPicker,
    map: fn(PickerMsg) -> Message,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let search = field(t!("symbol_picker.search"), &picker.query)
        .id(SEARCH_FIELD)
        .on_input(move |typed| map(PickerMsg::Query(typed)))
        .on_submit(map(PickerMsg::Ok));
    let main = row![
        container(shelves(picker, map, tokens))
            .width(SHELVES_WIDTH)
            .height(Length::Fill),
        container(grid(picker, map, tokens))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .spacing(16)
    .height(Length::Fill);
    let buttons = Row::new()
        .push(container(preview(picker, tokens)).width(Length::Fill))
        .push(layout::primary_icon_button(
            Icon::Confirm,
            t!("symbol_picker.ok"),
            picker.chosen().map(|_| map(PickerMsg::Ok)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Close,
            t!("symbol_picker.cancel"),
            Some(map(PickerMsg::Cancel)),
        ))
        .spacing(12)
        .align_y(Alignment::Center);
    column![
        fit::label(t!("symbol_picker.title")).font(theme::UI_FONT_STRONG),
        search,
        main,
        fit::help_line(t!("symbol_picker.keys"), tokens),
        buttons,
    ]
    .spacing(12)
    .into()
}

/// The shelves, one stop of the ring whose arrows choose the shelf before and after.
fn shelves<'a>(
    picker: &SymbolPicker,
    map: fn(PickerMsg) -> Message,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let list = Shelf::list();
    let active = picker.focus == PickFocus::Shelves;
    let mut lines = Column::new().spacing(1);
    for shelf in &list {
        let current = picker.shelf == *shelf;
        let line = container(fit::label(shelf_name(*shelf)))
            .padding([0, 6])
            .width(Length::Fill)
            .height(theme::ROW_HEIGHT)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| theme::striped_row(tokens, current, active, false, false));
        // focus-ring: pointer only (a row of the shelves, which are one stop of the ring)
        lines = lines.push(mouse_area(line).on_press(map(PickerMsg::Shelf(*shelf))));
    }
    let boxed = container(scrollable(lines).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active));
    let (previous, next) = focus::neighbours(&list, Some(&picker.shelf));
    let previous = previous.map(|shelf| map(PickerMsg::Shelf(*shelf)));
    let next = next.map(|shelf| map(PickerMsg::Shelf(*shelf)));
    focus::Stop::choice(boxed, SHELVES_STOP_ID, previous, next)
        .id(SHELVES_STOP_ID)
        .into()
}

/// The symbols shown, [`COLUMNS`] a row, the cursor's filled; one stop of the ring whose arrows
/// the picker takes (`symbol_picker::key`).
fn grid<'a>(
    picker: &SymbolPicker,
    map: fn(PickerMsg) -> Message,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let size = f32::from(picker.size);
    let cell = size + CELL_PADDING;
    let active = picker.focus == PickFocus::Grid;
    let body: Element<'a, Message> = if picker.shown.is_empty() {
        let words = if picker.shelf == Shelf::Recent && picker.query.trim().is_empty() {
            t!("symbol_picker.no_recent")
        } else {
            t!("symbol_picker.no_match")
        };
        container(fit::hint(words, tokens)).padding(8).into()
    } else {
        let all = symbols::all();
        let mut rows = Column::new().spacing(2);
        for (row_at, chunk) in picker.shown.chunks(COLUMNS).enumerate() {
            let mut line = Row::new().spacing(2);
            for (column_at, at) in chunk.iter().enumerate() {
                let Some(symbol) = all.get(*at) else {
                    continue;
                };
                let place = row_at * COLUMNS + column_at;
                let cursor = picker.cursor == Some(place);
                let face = container(glyph(symbol.glyph, size))
                    .width(cell)
                    .height(cell)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(move |_| theme::striped_row(tokens, cursor, active, false, false));
                // focus-ring: pointer only (a symbol of the grid, which is one stop of the ring)
                let pressed = mouse_area(face)
                    .on_press(map(PickerMsg::Select(place)))
                    .on_double_click(map(PickerMsg::Choose(place)));
                line = line.push(pressed);
            }
            rows = rows.push(line);
        }
        scrollable(rows)
            .id(GRID_SCROLL_ID)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };
    let boxed = container(body)
        .padding(4)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active));
    focus::Stop::choice(boxed, GRID_STOP_ID, None, None)
        .id(GRID_STOP_ID)
        .into()
}

/// The symbol at the cursor: large, as the button shows it, its name and the button's tooltip.
fn preview<'a>(picker: &SymbolPicker, tokens: &'static Tokens) -> Element<'a, Message> {
    let Some(symbol) = picker.chosen() else {
        return Space::new().into();
    };
    let size = f32::from(picker.size);
    let button = container(glyph(symbol.glyph, size))
        .padding(5)
        .style(move |_| button_style(tokens, false));
    let tooltip = if picker.tooltip.is_empty() {
        t!("symbol_picker.no_tooltip").to_owned()
    } else {
        picker.tooltip.clone()
    };
    let words = column![
        fit::label(symbol.name.to_owned()).font(theme::UI_FONT_STRONG),
        fit::cut(
            tooltip,
            theme::UI_FONT,
            Some(tokens.text_secondary),
            1,
            tokens
        ),
    ]
    .spacing(2)
    .width(Length::Fill);
    row![glyph(symbol.glyph, PREVIEW_SIZE), button, words]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}
