//! Drawing the Markdown preview (RC-020-09): iced's own `markdown` view in the theme's colours,
//! prose in the interface font at the viewer's size, code in the viewer's font (RC-010-17), both at
//! the window's text size (RC-020-12). A
//! scrollable holds it; the scrolling keys reach it by its id ([`super::markdown::scroll_id`]).
//!
//! Three of iced's defaults are drawn here instead: an image is never loaded and shows its alt
//! text in a frame; a code block takes the theme's surface rather than iced's fixed dark one; a
//! paragraph with links names their targets in a tooltip, a link that never opens marked so.
//!
//! While selecting is on (RC-020-14) each paragraph line, heading and code line is a
//! [`Selectable`] block, numbered as `select::blocks` numbers them, and the selected bytes carry
//! the theme's selection colour.

use std::{cell::Cell, sync::Arc};

use iced::{
    Background, Border, Element, Font, Length, Padding, Pixels, padding,
    widget::{
        Column, column, container,
        markdown::{self, HeadingLevel, Highlight, Settings, Style, Text, Uri, Viewer},
        rich_text, scrollable,
        scrollable::{Direction, Scrollbar},
        sensor,
    },
};

use super::{
    ViewerMessage, links,
    lister::Lister,
    markdown::{PreviewMessage, scroll_id},
    select::{self, Gesture, Selection, Span},
    selectable::Selectable,
    wrap,
};
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, fit, fonts::AreaFont},
};

const PADDING: f32 = 12.0;

/// Lister `lister`'s preview of `items`; a clicked link comes back as a message.
pub fn view<'a>(
    lister: &'a Lister,
    items: &'a [markdown::Item],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let id = lister.id;
    let look = Look {
        tokens,
        selection: lister.selection(),
        next: Cell::new(0),
    };
    let shown =
        markdown::view_with(items, settings(tokens, lister.font()), &look).map(move |shown| {
            let message = match shown {
                Shown::Link(url) => PreviewMessage::Link(url),
                Shown::Select(gesture) => PreviewMessage::Select(gesture),
            };
            wrap(ViewerMessage::Preview(id, message))
        });
    // Where it is scrolled to, so that a new text size keeps the place (RC-020-12).
    let page = scrollable(container(shown).padding(PADDING).width(Length::Fill))
        .id(scroll_id(id))
        .on_scroll(move |viewport| {
            let at = viewport.relative_offset().y;
            wrap(ViewerMessage::Preview(id, PreviewMessage::Scrolled(at)))
        })
        .width(Length::Fill)
        .height(Length::Fill);
    // The rows a page key scrolls, measured as the text view measures them.
    let measured = sensor(page)
        .on_show(move |size| wrap(ViewerMessage::Viewport(id, size)))
        .on_resize(move |size| wrap(ViewerMessage::Viewport(id, size)));
    container(measured)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| surface(tokens))
        .into()
}

/// The sizes, fonts and colours of the preview, from the theme and `viewer`, the viewer's font at
/// its window's size.
pub fn settings(tokens: &Tokens, viewer: AreaFont) -> Settings {
    let style = Style {
        font: theme::UI_FONT,
        inline_code_highlight: Highlight {
            background: Background::Color(tokens.header),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
        },
        inline_code_padding: Padding {
            top: 0.0,
            right: 2.0,
            bottom: 0.0,
            left: 2.0,
        },
        inline_code_color: tokens.text,
        inline_code_font: viewer.font,
        code_block_font: viewer.font,
        link_color: tokens.accent,
    };
    let mut settings = Settings::with_text_size(viewer.size, style);
    settings.code_size = viewer.size.into();
    settings
}

/// What the preview reports: a link clicked, or the mouse selecting.
#[derive(Debug, Clone)]
enum Shown {
    Link(Uri),
    Select(Gesture),
}

/// How the preview draws what iced's defaults would draw wrong here.
struct Look<'s> {
    tokens: &'static Tokens,
    /// `None` while selecting is off.
    selection: Option<&'s Selection>,
    /// The number of the next block.
    next: Cell<usize>,
}

impl<'a> Look<'a> {
    /// `text` in `size`, one block a line while selecting is on.
    fn text(
        &self,
        style: Style,
        text: &Text,
        size: Pixels,
        font: Option<Font>,
    ) -> Element<'a, Shown> {
        let spans = text.spans(style);
        // Without a hard break the block draws iced's cached spans, never a copy of them.
        if !spans.iter().any(|span| span.text.contains('\n')) {
            return self.block(spans, size, font);
        }
        let mut lines: Vec<Element<'a, Shown>> = select::lines(&spans)
            .into_iter()
            .map(|spans| self.block(spans.into(), size, font))
            .collect();
        if lines.len() == 1
            && let Some(line) = lines.pop()
        {
            return line;
        }
        Column::with_children(lines).into()
    }

    /// One line of text, numbered and marked where it is selected.
    fn block(&self, spans: Arc<[Span]>, size: Pixels, font: Option<Font>) -> Element<'a, Shown> {
        let block = self.next.get();
        self.next.set(block + 1);
        let selection = self.selection.cloned().unwrap_or_default();
        let len = spans.iter().map(|span| span.text.len()).sum();
        let drawn = match selection.range_in(block, len) {
            Some(range) => select::marked(&spans, range, selected(self.tokens)).into(),
            None => Arc::clone(&spans),
        };
        let mut content = rich_text(drawn).size(size).on_link_click(Shown::Link);
        if let Some(font) = font {
            content = content.font(font);
        }
        Selectable {
            content: content.into(),
            spans,
            size,
            font,
            block,
            dragging: selection.dragging,
            releases: selection.anchor_block() == Some(block),
            on_select: Shown::Select,
        }
        .into()
    }
}

impl<'a> Viewer<'a, Shown> for Look<'a> {
    fn on_link_click(url: Uri) -> Shown {
        Shown::Link(url)
    }

    /// Never loaded: the alt text in a frame.
    fn image(
        &self,
        settings: Settings,
        _url: &'a Uri,
        _title: &'a str,
        alt: &Text,
    ) -> Element<'a, Shown> {
        let tokens = self.tokens;
        let alt: Element<'a, Shown> = rich_text(alt.spans(settings.style))
            .size(settings.text_size)
            .into();
        container(alt)
            .padding(settings.spacing.0 / 2.0)
            .style(move |_| code_frame(tokens))
            .into()
    }

    fn heading(
        &self,
        settings: Settings,
        level: &'a HeadingLevel,
        text: &'a Text,
        index: usize,
    ) -> Element<'a, Shown> {
        if self.selection.is_none() {
            return markdown::heading(settings, level, text, index, Self::on_link_click);
        }
        let size = match level {
            HeadingLevel::H1 => settings.h1_size,
            HeadingLevel::H2 => settings.h2_size,
            HeadingLevel::H3 => settings.h3_size,
            HeadingLevel::H4 => settings.h4_size,
            HeadingLevel::H5 => settings.h5_size,
            HeadingLevel::H6 => settings.h6_size,
        };
        // The gap iced's own heading keeps above every heading but the first.
        let gap = if index > 0 {
            settings.text_size / 2.0
        } else {
            Pixels::ZERO
        };
        container(self.text(settings.style, text, size, None))
            .padding(padding::top(gap))
            .into()
    }

    fn paragraph(&self, settings: Settings, text: &Text) -> Element<'a, Shown> {
        let shown = if self.selection.is_some() {
            self.text(settings.style, text, settings.text_size, None)
        } else {
            markdown::paragraph(settings, text, Self::on_link_click)
        };
        let mut targets: Vec<String> = Vec::new();
        for span in text.spans(settings.style).iter() {
            if let Some(url) = &span.link
                && !targets.iter().any(|target| target == url)
            {
                targets.push(url.clone());
            }
        }
        if targets.is_empty() {
            return shown;
        }
        fit::tip(shown, link_tip(&targets), self.tokens)
    }

    fn code_block(
        &self,
        settings: Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [Text],
    ) -> Element<'a, Shown> {
        let tokens = self.tokens;
        let font = settings.style.code_block_font;
        let lines: Column<'a, Shown> = column(lines.iter().map(|line| {
            if self.selection.is_some() {
                return self.text(settings.style, line, settings.code_size, Some(font));
            }
            rich_text(line.spans(settings.style))
                .font(font)
                .size(settings.code_size)
                .into()
        }));
        let sideways = scrollable(container(lines).padding(settings.code_size.0 / 2.0))
            .direction(Direction::Horizontal(Scrollbar::default()));
        container(sideways)
            .width(Length::Fill)
            .padding(settings.code_size.0 / 4.0)
            .style(move |_| code_frame(tokens))
            .into()
    }
}

/// The tooltip of a paragraph's links: one target a line, one that never opens marked.
pub fn link_tip(targets: &[String]) -> String {
    targets
        .iter()
        .map(|url| {
            if links::opens(url) {
                links::shown(url)
            } else {
                t!("viewer.link_never_opens", url = links::shown(url))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn surface(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.surface)),
        border: Border {
            color: tokens.stroke,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

/// The selected text's background: the text editor's selection colour.
fn selected(tokens: &Tokens) -> Highlight {
    Highlight {
        background: Background::Color(iced::Color {
            a: 0.35,
            ..tokens.accent
        }),
        border: Border::default(),
    }
}

fn code_frame(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.header)),
        border: Border {
            color: tokens.stroke,
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

#[cfg(test)]
#[path = "markdown_view_tests.rs"]
mod tests;
