//! Drawing the diff window over the whole window: the options, both paths, then the rows on
//! screen - line numbers and text of each side, tinted by what differs - and the row of buttons
//! (the jumps at the left, Close at the right). A pair of binary or large files shows the byte
//! comparison instead.

use iced::{
    Alignment, Background, Color, Element, Length,
    alignment::Horizontal,
    widget::{Column, Space, center, checkbox, column, container, mouse_area, row, sensor, text},
};
use rc_engine::{
    VfsPath,
    compare::{FileComparison, RowKind, TextDiff, TextFile},
    shown_path,
};

use super::{CompareMsg, DiffOption, DiffWindow, Shown, wrap};
use crate::{
    app::Message,
    format,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
        icon::Icon,
        layout,
    },
};

/// The height of a row of the diff.
const LINE_HEIGHT: f32 = 20.0;
const NUMBER_WIDTH: f32 = 52.0;
/// Characters of a line drawn at most: a minified file's one line must not cost a frame.
const MAX_CHARS: usize = 2000;
/// The spaces a tab is drawn as.
const TAB: &str = "    ";

/// The rows that fit a list of `height`.
pub fn rows_for(height: f32) -> usize {
    (height / LINE_HEIGHT).floor().max(1.0) as usize
}

/// The window over everything else.
pub fn overlay<'a>(window: &'a DiffWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let head = Message::titled_head(t!("compare.title"));
    theme::large_dialog(head, body(window, tokens), tokens)
}

/// The options, the paths, the rows and the buttons: the card's content over the window, the
/// whole of the compare window of its own (RC-080-15).
pub fn body<'a>(window: &'a DiffWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let options = &window.options;
    let option = |label: &'static str, on: bool, which: DiffOption| {
        layout::check(label, on, true, move |on| {
            wrap(CompareMsg::DiffOption(which, on))
        })
    };
    let text_view = matches!(window.shown, Shown::Text(_));
    let tools = row![
        option(
            t!("compare.ignore_whitespace"),
            options.ignore_whitespace,
            DiffOption::Whitespace
        ),
        option(
            t!("compare.ignore_case"),
            options.ignore_case,
            DiffOption::Case
        ),
        option(
            t!("compare.ignore_line_endings"),
            options.ignore_line_endings,
            DiffOption::LineEndings
        ),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let buttons = layout::button_row(
        None,
        [layout::secondary_icon_button(
            Icon::Close,
            t!("compare.close"),
            Some(wrap(CompareMsg::DiffClose)),
        )],
        [
            button(
                t!("compare.previous"),
                text_view.then_some(CompareMsg::DiffPrevious),
            ),
            button(
                t!("compare.next"),
                text_view.then_some(CompareMsg::DiffNext),
            ),
        ],
    );
    let paths = row![path(&window.left, tokens), path(&window.right, tokens)].spacing(8);
    let body: Element<'a, Message> = match &window.shown {
        Shown::Loading => center(line(t!("compare.loading"))).into(),
        Shown::Failed(code) => center(fit::prose(error_text(code)).color(theme::DANGER)).into(),
        Shown::Bytes(comparison) => bytes(comparison),
        Shown::Text(diff) => rows(window, diff, tokens),
    };
    column![tools, status(window, tokens), paths, body, buttons]
        .spacing(8)
        .into()
}

/// One side's path or folder in half the width, shortened in the middle.
pub(super) fn path<'a>(path: &VfsPath, tokens: &'static Tokens) -> Element<'a, Message> {
    fit::path_line(shown_path(path).display().to_string(), tokens.text, tokens)
}

/// How many differences, or none.
fn status<'a>(window: &DiffWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let text = match &window.shown {
        Shown::Text(_) if window.changes == 0 => t!("compare.no_differences").to_owned(),
        Shown::Text(_) => t!(
            "compare.differences",
            count = format::count(window.changes as u64)
        ),
        _ => String::new(),
    };
    line(text).color(tokens.accent).into()
}

/// The byte comparison: both sizes, and identical or where they part.
fn bytes<'a>(comparison: &FileComparison) -> Element<'a, Message> {
    let sizes = t!(
        "compare.sizes",
        left = format::size(comparison.left_size),
        right = format::size(comparison.right_size)
    );
    let verdict = match comparison.first_difference {
        None => t!("compare.identical").to_owned(),
        Some(difference) => t!(
            "compare.first_difference",
            offset = format::count(difference.offset),
            hex = format!("0x{:X}", difference.offset),
            left = byte(difference.left),
            right = byte(difference.right)
        ),
    };
    center(column![line(t!("compare.binary")), line(sizes), line(verdict)].spacing(8)).into()
}

fn byte(value: Option<u8>) -> String {
    value.map_or_else(|| t!("compare.ended").to_owned(), |b| format!("0x{b:02X}"))
}

/// The rows on screen, the wheel scrolling them; the list reports its size.
fn rows<'a>(
    window: &'a DiffWindow,
    diff: &'a TextDiff,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let palette = theme::iced_theme(tokens).palette();
    let rows = window.visible().map(|index| {
        let shown = diff.rows[index];
        let (left, right) = match shown.kind {
            RowKind::Equal => (None, None),
            RowKind::Changed => (Some(palette.warning), Some(palette.warning)),
            RowKind::Deleted => (Some(palette.danger), Some(tokens.stroke)),
            RowKind::Inserted => (Some(tokens.stroke), Some(palette.success)),
        };
        let current = window.current == Some(index);
        let marker = container(Space::new().width(3.0).height(LINE_HEIGHT)).style(move |_| {
            container::Style {
                background: current.then_some(Background::Color(tokens.accent)),
                ..container::Style::default()
            }
        });
        row![
            marker,
            cell(&diff.left, shown.left, left, tokens),
            cell(&diff.right, shown.right, right, tokens),
        ]
        .spacing(4)
        .into()
    });
    let list = container(Column::with_children(rows))
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true);
    let list = mouse_area(list).on_scroll(|delta| {
        wrap(CompareMsg::DiffScroll(theme::wheel::rows(
            delta,
            LINE_HEIGHT,
        )))
    });
    let list = sensor(list)
        .on_show(|size| wrap(CompareMsg::DiffViewport(size)))
        .on_resize(|size| wrap(CompareMsg::DiffViewport(size)));
    // The rows are one stop of the ring; the window scrolls them with the arrows.
    focus::choice(list, "compare.rows", None, None)
}

/// One side of a row: the line's number and text, on its tint.
fn cell<'a>(
    file: &'a TextFile,
    line_index: Option<usize>,
    tint: Option<Color>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (number, content) = match line_index.and_then(|index| file.lines.get(index)) {
        Some(found) => (
            line_index.map_or_else(String::new, |index| (index + 1).to_string()),
            drawn(&found.text),
        ),
        None => (String::new(), String::new()),
    };
    let content = row![
        line(number)
            .color(tokens.text_secondary)
            .width(NUMBER_WIDTH)
            .align_x(Horizontal::Right),
        text(content)
            .size(theme::TEXT_SIZE)
            .font(theme::MONO_FONT)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    container(content)
        .width(Length::FillPortion(1))
        .height(LINE_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| container::Style {
            background: tint.map(|color| Background::Color(color.scale_alpha(0.22))),
            ..container::Style::default()
        })
        .into()
}

/// A line as drawn: tabs as spaces, cut at [`MAX_CHARS`].
fn drawn(line: &str) -> String {
    line.chars()
        .take(MAX_CHARS)
        .collect::<String>()
        .replace('\t', TAB)
}

/// A dialog button (`docs/design.md`); `None` draws it disabled.
pub(super) fn button<'a>(label: &'static str, message: Option<CompareMsg>) -> Element<'a, Message> {
    layout::secondary_button(label, message.map(wrap))
}

/// A checkbox, a stop of the focus ring (RC-080-16), whose label is no catalogue text of its own
/// (a count in it) and has no access key; `theme::layout::check` draws one that has.
pub(super) fn flag<'a>(
    label: impl text::IntoFragment<'a>,
    on: bool,
    message: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    let label = label.into_fragment();
    let toggled = message(!on);
    let check = checkbox(on)
        .label(label.clone())
        .on_toggle(message)
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT);
    focus::toggle(check, &label, Some(toggled))
}
