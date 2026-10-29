//! Drawing the About page: the two tabs, then the build or the licences.

use iced::{
    Alignment, ContentFit, Element, Length,
    widget::{Column, Row, button, column, container, image, row, scrollable, text, text_input},
};
use rc_engine::about::{About, AboutLink, AboutLinkKind};

use super::{AboutMsg, AboutPage, AboutTab, TABS, about_message, facts, licenses, logo, shown};
use crate::{
    app::{
        Message,
        settings::view::{hint, label},
    },
    i18n::t,
    theme::{self, Tokens, focus, icon::Icon, layout, page_tabs},
};

/// The width of the label column of the facts and the addresses.
const LABEL_WIDTH: f32 = 130.0;

/// The logo's size on the page: its PNG is 1360 by 360.
const LOGO_WIDTH: f32 = 272.0;
const LOGO_HEIGHT: f32 = 72.0;

/// The page: its tabs, then the chosen one.
pub fn view<'a>(page: &'a AboutPage, tokens: &'static Tokens) -> Element<'a, Message> {
    // The tabs are one stop of the ring; the arrows choose (RC-080-16, RC-100-20).
    let tabs = page_tabs::strip(
        &TABS.map(|tab| (tab, title(tab))),
        page.tab,
        "settings.about.tabs",
        |tab| about_message(AboutMsg::Tab(tab)),
        tokens,
    );
    let about = shown();
    let body: Element<'_, Message> = match page.tab {
        AboutTab::About => {
            scrollable(about_tab(about, &page.update_facts, tokens).padding([0, 12]))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
        AboutTab::Licenses => licenses_tab(&about, &page.filter, tokens),
    };
    column![tabs, body].spacing(12).height(Length::Fill).into()
}

fn title(tab: AboutTab) -> &'static str {
    match tab {
        AboutTab::About => t!("settings.about.tab.about"),
        AboutTab::Licenses => t!("settings.about.tab.licenses"),
    }
}

/// The logo, the build, the addresses and the credits.
fn about_tab<'a>(
    about: About,
    update_facts: &[(&'static str, String)],
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let mut page = Column::new().spacing(16);
    if let Some(logo) = logo::logo(tokens.dark) {
        page = page.push(
            image(logo)
                .width(LOGO_WIDTH)
                .height(LOGO_HEIGHT)
                .content_fit(ContentFit::Contain),
        );
    }
    let mut build = Column::new().spacing(4);
    for (name, value) in facts(&about) {
        build = build.push(fact(name, value, tokens));
    }
    for &(name, ref value) in update_facts {
        build = build.push(fact(name, value.clone(), tokens));
    }
    build = build.push(
        container(layout::keyed(
            focus::Stop::button(
                button(layout::marked_icon_button(
                    Icon::Copy,
                    t!("settings.about.copy_version"),
                ))
                .padding([6, 16])
                .style(button::secondary)
                .on_press(about_message(AboutMsg::CopyVersion)),
                t!("settings.about.copy_version"),
                Some(about_message(AboutMsg::CopyVersion)),
            ),
            t!("settings.about.copy_version"),
        ))
        .padding([6, 0]),
    );
    let mut links = Column::new()
        .spacing(4)
        .push(label(t!("settings.about.links")).font(theme::UI_FONT_STRONG));
    for link in about.links {
        links = links.push(address(link, tokens));
    }
    let credits = column![
        fact(
            t!("settings.about.author"),
            about.authors.join(", "),
            tokens
        ),
        fact(t!("settings.about.license"), about.license, tokens),
        prose(hint(t!("settings.about.thanks"), tokens)),
    ]
    .spacing(4);
    page.push(build).push(links).push(credits)
}

/// One fact: its label in the label column, its value beside it.
fn fact<'a>(name: &'static str, value: String, tokens: &'static Tokens) -> Element<'a, Message> {
    row![
        prose(label(name).color(tokens.text_secondary)).width(LABEL_WIDTH),
        label(value).wrapping(text::Wrapping::Glyph),
    ]
    .spacing(8)
    .into()
}

/// An address: a published one is copied with a click; one not published yet is its text with
/// the mark, or the mark alone while the address is not decided.
fn address<'a>(link: AboutLink, tokens: &'static Tokens) -> Element<'a, Message> {
    let name = match link.kind {
        AboutLinkKind::Source => t!("settings.about.link.source"),
        AboutLinkKind::Handbook => t!("settings.about.link.handbook"),
        AboutLinkKind::Changelog => t!("settings.about.link.changelog"),
    };
    let value: Element<'a, Message> = match (link.url, link.published) {
        (Some(url), true) => {
            let copy = about_message(AboutMsg::CopyLink(url.clone()));
            let link = button(label(url.clone()).color(tokens.accent))
                .padding(0)
                .style(button::text)
                .on_press(copy.clone());
            focus::button(link, &url, Some(copy))
        }
        (Some(url), false) => column![
            label(url).wrapping(text::Wrapping::Glyph),
            hint(t!("settings.about.unpublished"), tokens),
        ]
        .spacing(2)
        .into(),
        (None, _) => hint(t!("settings.about.unpublished"), tokens).into(),
    };
    row![
        prose(label(name).color(tokens.text_secondary)).width(LABEL_WIDTH),
        value
    ]
    .spacing(8)
    .into()
}

/// rCommander's own licence, the count per licence, the filter and the crates.
fn licenses_tab<'a>(
    about: &About,
    filter: &'a str,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let total = licenses::packages().len();
    let summary = licenses::counts()
        .iter()
        .map(|(license, count)| hint(format!("{license} \u{b7} {count}"), tokens).into());
    let rows = licenses::matching(filter).map(|package| {
        table_row(
            [
                (package.name.as_str(), 3, false),
                (package.version.as_str(), 2, true),
                (package.license.as_str(), 4, false),
            ],
            tokens,
        )
    });
    let shown = licenses::matching(filter).count();
    let header = table_row(
        [
            (t!("settings.about.licenses.name"), 3, true),
            (t!("settings.about.licenses.version"), 2, true),
            (t!("settings.about.licenses.license"), 4, true),
        ],
        tokens,
    );
    column![
        prose(label(t!(
            "settings.about.licenses.own",
            license = about.license
        ))),
        prose(hint(t!("settings.about.licenses.help"), tokens)),
        Row::with_children(summary).spacing(12).wrap(),
        row![
            text_input(t!("settings.about.licenses.filter"), filter)
                .on_input(|filter| about_message(AboutMsg::Filter(filter)))
                .padding([5, 8])
                .size(theme::TEXT_SIZE)
                .font(theme::UI_FONT)
                .width(240),
            hint(
                t!(
                    "settings.about.licenses.count",
                    shown = shown,
                    total = total
                ),
                tokens
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        container(header).style(move |_| theme::header(tokens)),
        scrollable(Column::with_children(rows)).height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

/// A row of the crate table: one line high, each cell cut at its column's end.
fn table_row<'a>(cells: [(&str, u16, bool); 3], tokens: &'static Tokens) -> Element<'a, Message> {
    let cells = cells.into_iter().map(|(value, portion, secondary)| {
        let color = if secondary {
            tokens.text_secondary
        } else {
            tokens.text
        };
        let shown = label(value.to_owned())
            .wrapping(text::Wrapping::None)
            .color(color);
        container(shown)
            .width(Length::FillPortion(portion))
            .clip(true)
            .into()
    });
    container(
        Row::with_children(cells)
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(theme::ROW_HEIGHT)
    .align_y(Alignment::Center)
    .clip(true)
    .into()
}

/// Prose wraps by word.
fn prose(text: text::Text<'_>) -> text::Text<'_> {
    text.wrapping(text::Wrapping::Word)
}
