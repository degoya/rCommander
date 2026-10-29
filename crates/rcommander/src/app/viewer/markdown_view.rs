//! Drawing the Markdown preview (RC-020-09): iced's own `markdown` view in the theme's colours,
//! prose in the interface font at the viewer's size, code in the viewer's font (RC-010-17), both at
//! the window's text size (RC-020-12). A
//! scrollable holds it; the scrolling keys reach it by its id ([`super::markdown::scroll_id`]).
//!
//! Three of iced's defaults are drawn here instead: an image is never loaded and shows its alt
//! text in a frame; a code block takes the theme's surface rather than iced's fixed dark one; a
//! paragraph with links names their targets in a tooltip, a link that never opens marked so.

use iced::{
    Background, Border, Element, Length, Padding,
    widget::{
        Column, column, container,
        markdown::{self, Highlight, Settings, Style, Text, Uri, Viewer},
        rich_text, scrollable,
        scrollable::{Direction, Scrollbar},
        sensor,
    },
};

use super::{
    ViewerMessage, links,
    lister::Lister,
    markdown::{PreviewMessage, scroll_id},
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
    lister: &Lister,
    items: &'a [markdown::Item],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let id = lister.id;
    let shown = markdown::view_with(items, settings(tokens, lister.font()), &Look { tokens })
        .map(move |url| wrap(ViewerMessage::Preview(id, PreviewMessage::Link(url))));
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

/// How the preview draws what iced's defaults would draw wrong here.
struct Look {
    tokens: &'static Tokens,
}

impl<'a> Viewer<'a, Uri> for Look {
    fn on_link_click(url: Uri) -> Uri {
        url
    }

    /// Never loaded: the alt text in a frame.
    fn image(
        &self,
        settings: Settings,
        _url: &'a Uri,
        _title: &'a str,
        alt: &Text,
    ) -> Element<'a, Uri> {
        let tokens = self.tokens;
        let alt: Element<'a, Uri> = rich_text(alt.spans(settings.style))
            .size(settings.text_size)
            .into();
        container(alt)
            .padding(settings.spacing.0 / 2.0)
            .style(move |_| code_frame(tokens))
            .into()
    }

    fn paragraph(&self, settings: Settings, text: &Text) -> Element<'a, Uri> {
        let shown = markdown::paragraph(settings, text, Self::on_link_click);
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
    ) -> Element<'a, Uri> {
        let tokens = self.tokens;
        let lines: Column<'a, Uri> = column(lines.iter().map(|line| {
            rich_text(line.spans(settings.style))
                .font(settings.style.code_block_font)
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
