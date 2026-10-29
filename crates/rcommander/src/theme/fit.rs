//! Text that stays in its container (`docs/design.md`, "Text never leaves its container"): one
//! line that never wraps, prose and help lines that wrap, a path shortened in the middle to the
//! width it is given, and the one-line cells and rows of a dialog's table, each with the whole
//! value in a tooltip.

use std::time::Duration;

use iced::{
    Alignment, Color, Element, Font, Length,
    widget::{Row, Tooltip, container, responsive, text, tooltip},
};

use super::{
    ROW_HEIGHT, TEXT_SIZE, Tokens, UI_FONT, dialog,
    ellipsis::{estimate_width, middle_ellipsis},
    striped_row,
};

/// One line at [`TEXT_SIZE`] in the interface font that never wraps: a label, a title, a value.
pub fn line<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content)
        .size(TEXT_SIZE)
        .font(UI_FONT)
        .wrapping(text::Wrapping::None)
}

/// Prose - an explanation, a note, an error: wraps by word in the width it has.
pub fn prose<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content)
        .size(TEXT_SIZE)
        .font(UI_FONT)
        .wrapping(text::Wrapping::Word)
        .width(Length::Fill)
}

/// A label beside a control, a line of a question: wraps by word, as wide as its content.
pub fn label<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content).size(TEXT_SIZE).font(UI_FONT)
}

/// [`label`] that breaks a word wider than its line between glyphs: a command, an argument, a
/// note with a path in it.
pub fn label_or_glyph<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    label(content).wrapping(text::Wrapping::WordOrGlyph)
}

/// A quiet line under a control: [`label`] at 12 in the secondary colour.
pub fn hint<'a>(content: impl text::IntoFragment<'a>, tokens: &Tokens) -> text::Text<'a> {
    label(content).size(12).color(tokens.text_secondary)
}

/// A dialog's keyboard help line: 12 in the secondary colour, wrapping, never cut.
pub fn help_line<'a>(content: impl text::IntoFragment<'a>, tokens: &Tokens) -> text::Text<'a> {
    prose(content).size(12).color(tokens.text_secondary)
}

/// `content` with `full` in a tooltip under it, in the dialog card's look.
pub fn tip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    full: String,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    tip_after(content, full, Duration::ZERO, tokens)
}

/// [`tip`] once the pointer has rested on `content` for `delay`.
pub fn tip_after<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    full: String,
    delay: Duration,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    Tooltip::new(content, line(full), tooltip::Position::Bottom)
        .gap(4)
        .padding(6)
        .delay(delay)
        .style(move |_| dialog(tokens))
        .into()
}

/// A path or a file name on one line in `color`, as wide as its container lets it be: shortened
/// in the middle when it does not fit, then the whole value in a tooltip.
pub fn path_line<'a, Message: 'a>(
    value: impl Into<String>,
    color: Color,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let value = value.into();
    responsive(move |size| fitted(&value, size.width, color, tokens))
        .height(Length::Shrink)
        .into()
}

/// A table cell holding a path, a file name or a host: [`path_line`] in a column of `portion`.
pub fn path_cell<'a, Message: 'a>(
    value: impl Into<String>,
    portion: u16,
    secondary: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let value = value.into();
    let color = cell_color(secondary, tokens);
    responsive(move |size| fitted(&value, size.width, color, tokens))
        .width(Length::FillPortion(portion))
        .height(Length::Shrink)
        .into()
}

/// A table cell of any other value: one line, cut at the column's end, the whole value in a
/// tooltip.
pub fn cell<'a, Message: 'a>(
    value: impl Into<String>,
    portion: u16,
    secondary: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let color = cell_color(secondary, tokens);
    cut(value, UI_FONT, Some(color), portion, tokens)
}

/// `value` on one line in `font` in a column of `portion`, cut at its width; the whole value in
/// a tooltip only when it does not fit (owner 2026-10-18: no tooltip that repeats what is shown).
pub fn cut<'a, Message: 'a>(
    value: impl Into<String>,
    font: Font,
    color: Option<Color>,
    portion: u16,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let value = value.into();
    responsive(move |size| {
        let mut shown = line(value.clone()).font(font);
        if let Some(color) = color {
            shown = shown.color(color);
        }
        let shown = container(shown).width(Length::Fill).clip(true);
        if overflows(&value, size.width) {
            tip(shown, value.clone(), tokens)
        } else {
            shown.into()
        }
    })
    .width(Length::FillPortion(portion))
    .height(Length::Shrink)
    .into()
}

/// Whether `value` at [`TEXT_SIZE`] is wider than `width` by the estimate (which errs wide).
pub fn overflows(value: &str, width: f32) -> bool {
    !value.is_empty() && estimate_width(value, TEXT_SIZE) > width
}

/// A row of a dialog's table: one line of [`ROW_HEIGHT`], its cells side by side 8 apart, on
/// the stripe's colour when `alternate` (`theme::alternate` of its index).
pub fn table_row<'a, Message: 'a>(
    alternate: bool,
    tokens: &'static Tokens,
    cells: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        Row::with_children(cells)
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(ROW_HEIGHT)
    .align_y(Alignment::Center)
    .clip(true)
    .style(move |_| striped_row(tokens, false, true, false, alternate))
    .into()
}

fn cell_color(secondary: bool, tokens: &Tokens) -> Color {
    if secondary {
        tokens.text_secondary
    } else {
        tokens.text
    }
}

/// `value` in `width`: the whole of it, or shortened with the whole value in a tooltip. The
/// container clips what the estimate of the width missed.
fn fitted<'a, Message: 'a>(
    value: &str,
    width: f32,
    color: Color,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (shown, full) = match shortened(value, width) {
        Some(shown) => (shown, Some(value.to_owned())),
        None => (value.to_owned(), None),
    };
    let shown = container(line(shown).color(color))
        .width(Length::Fill)
        .clip(true);
    match full {
        Some(full) => tip(shown, full, tokens),
        None => shown.into(),
    }
}

/// `value` shortened in the middle to `width` at [`TEXT_SIZE`], or `None` when it fits.
fn shortened(value: &str, width: f32) -> Option<String> {
    let shown = middle_ellipsis(value, width, TEXT_SIZE);
    (shown != value).then(|| shown.into_owned())
}

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;
