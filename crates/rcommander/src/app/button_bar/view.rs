//! Drawing the bars (RC-100-87): the button bar under the menu bar, the vertical one between the
//! panels, and the open menu over the window. A button shows an icon at the bars' size
//! (`buttonbar.icon_size`), as Total Commander's bar does: its symbol file or a symbol of the
//! collection (`rc-symbol:<name>`, RC-110-10), else an own command's symbol, else for
//! rCommander's commands a glyph of the app's icon font (`glyphs.rs`), else on
//! Windows a program's own icon. Only a button with no icon at all shows text - its tooltip, else
//! its command's name (an own command's tooltip, a program's file name) - in the middle cut to
//! [`MAX_TEXT`] (`theme::ellipsis`); the tooltip always names it in full. A sub-bar's button
//! carries `▾`. The button at the keyboard's cursor has the accent's border. What the bar has no
//! room for goes behind `»`, as in the drive bar (`docs/design.md`, Bars): nothing is cut off
//! unseen. The buttons' measures and look - the room around the symbol, flat or framed - are
//! `layout.rs`'s (RC-110-15); a right click beside the buttons opens the bar's context menu. The
//! menus are drawn by `menu_view.rs`.

use iced::{
    Alignment, Element, Length,
    advanced::widget::Id,
    mouse::Interaction,
    widget::{
        Column, Row, Space, container,
        image::{Handle, Image},
        mouse_area, responsive, text,
        tooltip::{self, Tooltip},
    },
};
use rc_engine::{BarButton, BarItem, BarPlace, ButtonAction, ButtonBar, CommandKind};

use super::{
    BbMsg, glyphs,
    layout::{self, Measures},
    symbols, wrap,
};
use crate::{
    app::{
        App, Message,
        drive_bar::{button_style, card_style},
        palette, user_commands,
    },
    i18n::t,
    theme::{self, Tokens, ellipsis},
};

/// The text size of a button without a symbol.
const SMALL: f32 = 12.0;
/// The widest a button's text gets before its middle is cut.
pub const MAX_TEXT: f32 = 120.0;
/// The space between two items.
const SPACING: f32 = 3.0;
/// A separator's room.
const GAP: f32 = 8.0;
/// The arrow of a sub-bar's button.
const SUB_ARROW: &str = "\u{25be}";
/// The overflow button.
const MORE: &str = "\u{bb}";

/// The container id of the bar at `place`, which a drop is measured against.
pub fn bar_id(place: BarPlace) -> Id {
    Id::from(format!("rcommander.button-bar.{}", place_name(place)))
}

/// The container id of the item at `index` of the bar at `place`.
pub fn item_id(place: BarPlace, index: usize) -> Id {
    Id::from(format!(
        "rcommander.button-bar.{}.{index}",
        place_name(place)
    ))
}

/// The container id of the `»` of the bar at `place`.
pub fn more_id(place: BarPlace) -> Id {
    Id::from(format!("rcommander.button-bar.{}.more", place_name(place)))
}

fn place_name(place: BarPlace) -> &'static str {
    match place {
        BarPlace::Horizontal => "horizontal",
        BarPlace::Vertical => "vertical",
    }
}

/// The words a button shows without a symbol and in its menu: its tooltip, else its command's.
pub fn label_of(button: &BarButton, app: &App) -> String {
    if !button.tooltip.is_empty() {
        return button.tooltip.clone();
    }
    match &button.action {
        ButtonAction::SubBar(id) => app
            .button_bar
            .bar(*id)
            .map_or_else(|| id.to_string(), |bar| bar.name.clone()),
        ButtonAction::Command(action) => match action.kind {
            CommandKind::Internal => {
                let rows = action.rows();
                let first = rows.first().copied().unwrap_or_default();
                let row = rc_engine::user_commands::tc_names::row_of(first).unwrap_or(first);
                palette::label(row)
            }
            // An own command without a tooltip shows the command's (Total Commander 11.50).
            CommandKind::Own => user_commands::known::with_commands(|commands| {
                commands
                    .iter()
                    .find(|command| command.name == action.command)
                    .filter(|command| !command.tooltip.is_empty())
                    .map_or_else(|| action.command.clone(), |command| command.tooltip.clone())
            }),
            CommandKind::Program => std::path::Path::new(&action.command)
                .file_stem()
                .map_or_else(
                    || action.command.clone(),
                    |stem| stem.to_string_lossy().into_owned(),
                ),
        },
    }
}

/// The symbol of `button` at the bars' size, once read.
pub(super) fn symbol<'a>(button: &BarButton, app: &'a App) -> Option<Element<'a, Message>> {
    let size = f32::from(app.button_bar.settings.icon_size);
    // An internal command's glyph, in the button's text colour of either theme.
    if let Some(icon) = glyphs::of(button) {
        return Some(glyph(icon.glyph(), theme::icon::FONT, size));
    }
    // A symbol of the collection, drawn alike (RC-110-10).
    if let Some(symbol) = glyphs::collection(button) {
        return Some(glyph(symbol, theme::symbols::FONT, size));
    }
    let key = symbols::key_of(button, app.button_bar.settings.icon_size)?;
    let handle: Handle = app.button_bar.symbols.get(&key)?.clone();
    Some(Image::<Handle>::new(handle).width(size).height(size).into())
}

/// `symbol` of `font` at `size`, in the text colour of the button it sits in.
fn glyph<'a>(symbol: char, font: iced::Font, size: f32) -> Element<'a, Message> {
    text(symbol)
        .font(font)
        .size(size)
        .line_height(text::LineHeight::Relative(1.0))
        .center()
        .width(size)
        .height(size)
        .into()
}

/// Whether `button` shows an icon: its symbol read, an internal command's glyph or a symbol of
/// the collection.
fn has_icon(button: &BarButton, app: &App) -> bool {
    glyphs::of(button).is_some()
        || glyphs::collection(button).is_some()
        || symbols::key_of(button, app.button_bar.settings.icon_size)
            .is_some_and(|key| app.button_bar.symbols.get(&key).is_some())
}

/// The face of `button`: its icon, or - only without one - its words cut to `room`; a sub-bar's
/// with its arrow.
fn face<'a>(button: &BarButton, app: &'a App, room: f32) -> Element<'a, Message> {
    let symbol = symbol(button, app);
    let mut parts = Row::new().spacing(2).align_y(Alignment::Center);
    parts = match symbol {
        Some(symbol) => parts.push(symbol),
        None => {
            let words = label_of(button, app);
            let cut = ellipsis::middle_ellipsis(&words, room, SMALL).into_owned();
            parts.push(small(cut))
        }
    };
    if matches!(button.action, ButtonAction::SubBar(_)) {
        parts = parts.push(small(SUB_ARROW));
    }
    parts.into()
}

pub(super) fn small<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content)
        .size(SMALL)
        .font(theme::UI_FONT)
        .wrapping(text::Wrapping::None)
}

/// The estimated width of the item at `index` of `bar` in a horizontal bar.
fn item_width(item: &BarItem, app: &App) -> f32 {
    match item {
        BarItem::Separator => GAP,
        BarItem::Button(button) => {
            let size = app.button_bar.settings.icon_size;
            let inner = if has_icon(button, app) {
                f32::from(size)
            } else {
                ellipsis::estimate_width(&label_of(button, app), SMALL).min(MAX_TEXT)
            };
            let arrow = if matches!(button.action, ButtonAction::SubBar(_)) {
                ellipsis::estimate_width(SUB_ARROW, SMALL) + 2.0
            } else {
                0.0
            };
            Measures::of(&app.button_bar.settings).button(inner + arrow)
        }
    }
}

/// The estimated height of an item in the vertical bar.
fn item_height(item: &BarItem, app: &App) -> f32 {
    match item {
        BarItem::Separator => GAP,
        BarItem::Button(_) => Measures::of(&app.button_bar.settings).vertical_height(SMALL + 4.0),
    }
}

/// How many of the items `sizes` long fit in `room`, [`SPACING`] apart: all, or as many as
/// leave room for `»` (`more` long) after them.
pub fn visible_count(sizes: &[f32], room: f32, more: f32) -> usize {
    let line =
        |items: &[f32]| items.iter().sum::<f32>() + SPACING * items.len().saturating_sub(1) as f32;
    if line(sizes) <= room {
        return sizes.len();
    }
    let room = room - more - SPACING;
    (0..sizes.len())
        .take_while(|&count| line(&sizes[..=count]) <= room)
        .count()
}

/// The item at `index` of `bar` at `place`: a button with its tooltip, a separator's gap.
fn item<'a>(
    app: &'a App,
    bar: &'a ButtonBar,
    place: BarPlace,
    index: usize,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(button) = bar.spec.items.get(index).and_then(BarItem::button) else {
        return match place {
            BarPlace::Horizontal => Space::new().width(GAP).into(),
            BarPlace::Vertical => Space::new().height(GAP).into(),
        };
    };
    let at = |focus: Option<super::BarFocus>| {
        focus.is_some_and(|focus| focus.place == place && focus.index == index)
    };
    let focused = at(app.button_bar.focus);
    let hovered = at(app.button_bar.hover);
    let settings = &app.button_bar.settings;
    let measures = Measures::of(settings);
    let room = match place {
        BarPlace::Horizontal => MAX_TEXT,
        BarPlace::Vertical => measures.icon.max(layout::MIN_VERTICAL_FACE),
    };
    let flat = settings.flat;
    let boxed = container(face(button, app, room))
        .id(item_id(place, index))
        .padding(measures.padding)
        .style(move |_| layout::button_style(tokens, flat, focused, hovered));
    // focus-ring: pointer only (the bar is no dialog: Alt+Shift+F11 and F12 give it the keys)
    let pressed = mouse_area(boxed)
        .on_press(wrap(BbMsg::Press(place, index)))
        .on_right_press(wrap(BbMsg::Context(place, Some(index))))
        .on_enter(wrap(BbMsg::Hover(place, index, true)))
        .on_exit(wrap(BbMsg::Hover(place, index, false)))
        .interaction(Interaction::Pointer);
    with_tooltip(pressed.into(), label_of(button, app), tokens)
}

/// `content` with `hint` shown under it on hover.
fn with_tooltip<'a>(
    content: Element<'a, Message>,
    hint: String,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let tip = container(small(hint)).padding([4, 8]);
    Tooltip::new(content, tip, tooltip::Position::Bottom)
        .gap(4)
        .style(move |_| card_style(tokens))
        .into()
}

/// The `»` of the bar at `place`: the items from `from` on as a menu.
fn more<'a>(place: BarPlace, from: usize, tokens: &'static Tokens) -> Element<'a, Message> {
    let boxed = container(small(MORE))
        .id(more_id(place))
        .padding([2, 7])
        .style(move |_| button_style(tokens, false));
    // focus-ring: pointer only (the bar is no dialog: its keys reach the items behind it too)
    let pressed = mouse_area(boxed)
        .on_press(wrap(BbMsg::More(place, from)))
        .interaction(Interaction::Pointer);
    with_tooltip(pressed.into(), t!("button_bar.more").to_owned(), tokens)
}

/// The button bar under the menu bar; `None` while it is off or has no bar to show.
pub fn horizontal<'a>(app: &'a App, tokens: &'static Tokens) -> Option<Element<'a, Message>> {
    let bar = app.button_bar.shown(BarPlace::Horizontal)?;
    let line = responsive(move |size| {
        let widths: Vec<f32> = bar
            .spec
            .items
            .iter()
            .map(|item| item_width(item, app))
            .collect();
        let more_width = ellipsis::estimate_width(MORE, SMALL) + 16.0;
        let shown = visible_count(&widths, size.width, more_width);
        let mut line = Row::with_children(
            (0..shown).map(|index| item(app, bar, BarPlace::Horizontal, index, tokens)),
        )
        .spacing(SPACING)
        .align_y(Alignment::Center);
        if shown < bar.spec.items.len() {
            line = line.push(more(BarPlace::Horizontal, shown, tokens));
        }
        container(line).width(Length::Fill).clip(true).into()
    })
    .height(Length::Shrink);
    let bar = container(line)
        .id(bar_id(BarPlace::Horizontal))
        .width(Length::Fill);
    Some(beside_buttons(bar.into(), BarPlace::Horizontal))
}

/// `bar` taking a right click beside its buttons: the bar's context menu (RC-110-15). A
/// button's own right click is the button's.
fn beside_buttons(bar: Element<'_, Message>, place: BarPlace) -> Element<'_, Message> {
    // focus-ring: pointer only (the bar is no dialog: its keys open the menu, Shift+F10)
    mouse_area(bar)
        .on_right_press(wrap(BbMsg::Context(place, None)))
        .into()
}

/// The vertical button bar between the panels; `None` while it is off or has no bar to show.
pub fn vertical<'a>(app: &'a App, tokens: &'static Tokens) -> Option<Element<'a, Message>> {
    let bar = app.button_bar.shown(BarPlace::Vertical)?;
    let width = Measures::of(&app.button_bar.settings).vertical_width();
    let line = responsive(move |size| {
        let heights: Vec<f32> = bar
            .spec
            .items
            .iter()
            .map(|item| item_height(item, app))
            .collect();
        let shown = visible_count(&heights, size.height, SMALL + 8.0);
        let mut line = Column::with_children(
            (0..shown).map(|index| item(app, bar, BarPlace::Vertical, index, tokens)),
        )
        .spacing(SPACING)
        .align_x(Alignment::Center);
        if shown < bar.spec.items.len() {
            line = line.push(more(BarPlace::Vertical, shown, tokens));
        }
        container(line).height(Length::Fill).clip(true).into()
    });
    let bar = container(line)
        .id(bar_id(BarPlace::Vertical))
        .width(width)
        .height(Length::Fill);
    Some(beside_buttons(bar.into(), BarPlace::Vertical))
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
