//! Drawing the viewer: Lister over the whole window (F3), Quick View in a panel's place
//! (Ctrl+Q). The text, binary and hex views are a read-only `text_editor` holding the page, a
//! scrollbar beside it that works by byte position; the image view is the image widget, fit or
//! zoomed and turned in the view only. The text views are in the viewer's chosen font
//! (RC-010-17) at the window's text size (RC-020-12), which sets the line height. A Markdown
//! file's preview is `markdown_view.rs`; a plugin document's tree and list of views
//! `tree_view.rs`, its coloured text the text view with the spans of `spans.rs` (RC-100-10).

use std::f32::consts::FRAC_PI_2;

use iced::{
    Alignment, Background, Border, ContentFit, Element, Length, Radians, Rotation,
    alignment::Horizontal,
    keyboard::Key,
    widget::{
        center, checkbox, column, container, image, opaque, row, scrollable,
        scrollable::{Direction, Scrollbar},
        sensor, stack,
        text::{LineHeight, Wrapping},
        text_editor,
        text_editor::{Binding, KeyPress, Status},
        text_input, vertical_slider,
    },
};
use rc_engine::{
    PanelId, VfsPath,
    viewer::{LineEnding, ViewMode},
};

use super::{
    ViewerMessage,
    find::{BarKind, FindOption},
    lister::{Lister, Shown, Zoom},
    quick::{DirSummary, QuickShown},
    spans::{self, SpanHighlighter},
    tree_view, wrap,
};
use crate::{
    app::{App, Message, file_list::divider},
    format,
    i18n::{self, t},
    theme::{
        self, Tokens,
        fit::{self, line},
    },
};

/// The id of the bar's field, to focus it.
pub const BAR_ID: &str = "rcommander.viewer.bar";
const PADDING: f32 = 8.0;
/// The scrollbar's steps over the whole file.
const SCROLL_STEPS: f32 = 1000.0;

/// The lines of `line_height` that fit a text view of `height`; one at least.
pub fn rows_at(height: f32, line_height: f32) -> usize {
    ((height - 2.0 * PADDING) / line_height).floor().max(1.0) as usize
}

impl App {
    /// The window, with the top viewer over it while one is open there.
    pub(crate) fn with_lister<'a>(
        &'a self,
        base: Element<'a, Message>,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        let Some(lister) = self.viewers.open.last().filter(|_| self.lister_over_main()) else {
            return base;
        };
        stack![base, opaque(filled(lister, tokens))].into()
    }

    /// Lister `id` filling a window of its own (RC-080-15), while it is open.
    pub(crate) fn lister_body(
        &self,
        id: u64,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let lister = self.viewers.open.iter().find(|lister| lister.id == id)?;
        Some(filled(lister, tokens))
    }

    /// The Quick View in the place of `side`'s file list, while it shows there.
    pub(crate) fn quick_view_body(
        &self,
        side: PanelId,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let quick = self.viewers.quick.as_ref().filter(|q| q.target() == side)?;
        Some(match &quick.shown {
            QuickShown::File(lister) => container(lister_view(lister, tokens, false))
                .padding(4)
                .into(),
            QuickShown::Dir { path, summary } => dir_view(path, summary.as_ref(), tokens),
            QuickShown::Nothing => center(line(t!("viewer.quick_nothing"))).into(),
        })
    }
}

/// A Lister over the whole of a window.
fn filled<'a>(lister: &'a Lister, tokens: &'static Tokens) -> Element<'a, Message> {
    container(lister_view(lister, tokens, true))
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::window(tokens))
        .into()
}

fn lister_view<'a>(
    lister: &'a Lister,
    tokens: &'static Tokens,
    full: bool,
) -> Element<'a, Message> {
    let body = match (lister.mode, lister.preview()) {
        _ if lister.tree_shown() => tree_view::view(lister, tokens),
        (ViewMode::Image, _) => picture_view(lister),
        (_, Some(items)) => super::markdown_view::view(lister, items, tokens),
        _ if lister.preview_waiting() => center(line(t!("viewer.loading"))).into(),
        _ => text_view(lister, tokens),
    };
    let title = tree_view::title(
        lister,
        line(lister.name()).font(theme::UI_FONT_STRONG).into(),
    );
    let mut parts: Vec<Element<'a, Message>> = vec![
        container(title)
            .padding([4, 8])
            .width(Length::Fill)
            .clip(true)
            .style(move |_| theme::header(tokens))
            .into(),
        body,
    ];
    if let Some(kind) = lister.find.bar {
        parts.push(bar(lister, kind));
    }
    parts.push(divider(tokens));
    let mut status = row![line(status_text(lister))].spacing(12);
    if full {
        // The keys wrap at the right rather than run out of the window.
        let hints = tree_view::hints(lister);
        status = status.push(fit::help_line(hints, tokens).align_x(Horizontal::Right));
    }
    parts.push(
        container(status.align_y(Alignment::Center))
            .padding([2, 8])
            .into(),
    );
    column(parts)
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn text_view<'a>(lister: &'a Lister, tokens: &'static Tokens) -> Element<'a, Message> {
    let id = lister.id;
    let font = lister.font();
    let wrapping = if lister.wrap {
        Wrapping::WordOrGlyph
    } else {
        Wrapping::None
    };
    // A document's coloured text paints its spans; any other text has none to paint.
    let highlights = lister
        .document
        .as_ref()
        .map(|document| document.spans.shown().clone())
        .unwrap_or_default();
    let editor = text_editor(&lister.content)
        .highlight_with::<SpanHighlighter>(highlights, spans::format)
        .on_action(move |action| wrap(ViewerMessage::Editor(id, action)))
        .key_binding(read_only)
        .font(font.font)
        .size(font.size)
        .line_height(LineHeight::Absolute(lister.line_height().into()))
        .padding(PADDING)
        .wrapping(wrapping)
        .height(Length::Fill)
        .style(move |_, _| editor_style(tokens));
    // Keyed by the line height: a new font counts the lines that fit again.
    let measured = sensor(editor)
        .on_show(move |size| wrap(ViewerMessage::Viewport(id, size)))
        .on_resize(move |size| wrap(ViewerMessage::Viewport(id, size)))
        .key(lister.line_height().to_bits());
    // A vertical slider has its maximum at the top; the file starts there.
    let position = SCROLL_STEPS * (1.0 - lister.share());
    let scrollbar = vertical_slider(0.0..=SCROLL_STEPS, position, move |value: f32| {
        wrap(ViewerMessage::Scrollbar(id, 1.0 - value / SCROLL_STEPS))
    })
    .width(12)
    .height(Length::Fill);
    row![measured, scrollbar]
        .spacing(4)
        .height(Length::Fill)
        .into()
}

/// The editor reads only: Ctrl+C copies and Ctrl+A selects while it has the keyboard, every
/// other key is the viewer's.
pub(in crate::app) fn read_only(press: KeyPress) -> Option<Binding<Message>> {
    if !matches!(press.status, Status::Focused { .. }) || !press.modifiers.command() {
        return None;
    }
    match press.key.as_ref() {
        Key::Character("c") => Some(Binding::Copy),
        Key::Character("a") => Some(Binding::SelectAll),
        _ => None,
    }
}

pub(in crate::app) fn editor_style(tokens: &Tokens) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(tokens.surface),
        border: Border {
            color: tokens.stroke,
            width: 1.0,
            radius: 4.0.into(),
        },
        placeholder: tokens.text_secondary,
        value: tokens.text,
        selection: iced::Color {
            a: 0.35,
            ..tokens.accent
        },
    }
}

fn picture_view(lister: &Lister) -> Element<'_, Message> {
    let picture = match &lister.picture {
        Shown::Ready(picture) => picture,
        Shown::Failed(code) => return center(line(i18n::error_text(code))).into(),
        Shown::Loading | Shown::None => return center(line(t!("viewer.loading"))).into(),
    };
    let Zoom { fit, scale, turns } = lister.zoom;
    let rotation = Rotation::Solid(Radians(FRAC_PI_2 * f32::from(turns)));
    if fit {
        let shown = image(picture.handle.clone())
            .content_fit(ContentFit::ScaleDown)
            .rotation(rotation)
            .width(Length::Fill)
            .height(Length::Fill);
        return center(shown).into();
    }
    let (width, height) = if turns % 2 == 1 {
        (picture.height, picture.width)
    } else {
        (picture.width, picture.height)
    };
    let shown = image(picture.handle.clone())
        .content_fit(ContentFit::Fill)
        .rotation(rotation)
        .width(width as f32 * scale)
        .height(height as f32 * scale);
    scrollable(shown)
        .direction(Direction::Both {
            vertical: Scrollbar::default(),
            horizontal: Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn bar(lister: &Lister, kind: BarKind) -> Element<'_, Message> {
    let id = lister.id;
    let (placeholder, value) = match kind {
        BarKind::Search => (t!("viewer.search_placeholder"), &lister.find.text),
        BarKind::GoTo => (t!("viewer.goto_placeholder"), &lister.find.offset),
    };
    let field = text_input(placeholder, value)
        .id(BAR_ID)
        .on_input(move |text| wrap(ViewerMessage::BarInput(id, text)))
        .on_submit(wrap(ViewerMessage::BarSubmit(id)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([4, 8]);
    let mut parts = row![field].spacing(12).align_y(Alignment::Center);
    if kind == BarKind::Search {
        let find = &lister.find;
        for (option, on, label) in [
            (FindOption::Case, find.case, t!("viewer.search_case")),
            (FindOption::Hex, find.hex, t!("viewer.search_hex")),
            (
                FindOption::Backward,
                find.backward,
                t!("viewer.search_backward"),
            ),
        ] {
            parts = parts.push(
                checkbox(on)
                    .label(label)
                    .text_size(theme::TEXT_SIZE)
                    .on_toggle(move |_| wrap(ViewerMessage::Toggle(id, option))),
            );
        }
    }
    container(parts).padding([2, 0]).into()
}

fn dir_view<'a>(
    path: &'a VfsPath,
    summary: Option<&'a Result<DirSummary, String>>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let name = path.path.file_name().map_or_else(
        || path.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let numbers = match summary {
        None => t!("viewer.loading").to_owned(),
        Some(Ok(summary)) => t!(
            "viewer.quick_dir",
            files = format::count(summary.files),
            dirs = format::count(summary.dirs),
            size = format::size(summary.bytes)
        ),
        Some(Err(message)) => message.clone(),
    };
    center(
        column![
            line(name).font(theme::UI_FONT_STRONG),
            line(numbers).color(tokens.text_secondary),
        ]
        .spacing(6)
        .align_x(Alignment::Center),
    )
    .into()
}

/// The status line: a search's progress, the last note, or what is shown and where.
fn status_text(lister: &Lister) -> String {
    if let Some(percent) = lister.search_percent() {
        return t!("viewer.searching", percent = percent);
    }
    if let Some(note) = &lister.note {
        return note.clone();
    }
    if let Some(tree) = tree_view::status(lister) {
        return tree;
    }
    let Some(file) = &lister.file else {
        return t!("viewer.loading").to_owned();
    };
    if lister.preview().is_some() {
        return t!("viewer.status_markdown", size = format::size(lister.size()));
    }
    if lister.mode == ViewMode::Image {
        return match &lister.picture {
            Shown::Ready(picture) => t!(
                "viewer.status_image",
                format = picture.format,
                width = picture.width,
                height = picture.height,
                zoom = zoom_text(lister.zoom)
            ),
            _ => mode_label(lister.mode).to_owned(),
        };
    }
    let percent = (lister.share() * 100.0).round() as u32;
    t!(
        "viewer.status_text",
        mode = mode_label(lister.mode),
        encoding = lister.detected.encoding.label(),
        ending = ending_label(file.ending),
        position = format::count(lister.top),
        size = format::size(lister.size()),
        percent = percent
    )
}

fn mode_label(mode: ViewMode) -> &'static str {
    match mode {
        ViewMode::Text => t!("viewer.mode_text"),
        ViewMode::Binary => t!("viewer.mode_binary"),
        ViewMode::Hex => t!("viewer.mode_hex"),
        ViewMode::Image => t!("viewer.mode_image"),
    }
}

fn ending_label(ending: Option<LineEnding>) -> &'static str {
    match ending {
        Some(LineEnding::CrLf) => t!("viewer.ending_crlf"),
        Some(LineEnding::Lf) => t!("viewer.ending_lf"),
        Some(LineEnding::Cr) => t!("viewer.ending_cr"),
        None => t!("viewer.ending_none"),
    }
}

fn zoom_text(zoom: Zoom) -> String {
    if zoom.fit {
        return t!("viewer.zoom_fit").to_owned();
    }
    t!(
        "viewer.zoom_percent",
        percent = (zoom.scale * 100.0).round()
    )
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
