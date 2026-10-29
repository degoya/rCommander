//! Drawing the Host keys page: the help line, the search, the table of trusted keys and
//! certificates - one stop of the focus ring - the question before a removal, and the page's
//! buttons.

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, container, mouse_area, row, scrollable, text_input},
};

use super::{
    host_keys::{HostKeysMsg, HostKeysPage, Stored, page_message},
    view::{hint, label},
};
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The columns' shares of the width: server, port, type, fingerprint, trusted since.
const PORTIONS: [u16; 5] = [4, 1, 3, 7, 3];

pub fn view<'a>(page: &'a HostKeysPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let search = text_input(t!("settings.host_keys.search"), &page.filter)
        .on_input(|filter| page_message(HostKeysMsg::Filter(filter)))
        .padding([5, 8])
        .size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .width(240);
    let count = page.entries.as_ref().map(|entries| {
        hint(
            t!("settings.host_keys.count", count = entries.len()),
            tokens,
        )
    });
    let header = fit::table_row(
        false,
        tokens,
        [
            t!("settings.host_keys.server"),
            t!("settings.host_keys.port"),
            t!("settings.host_keys.type"),
            t!("settings.host_keys.fingerprint"),
            t!("settings.host_keys.added"),
        ]
        .into_iter()
        .zip(PORTIONS)
        .map(|(title, portion)| fit::cell(title, portion, true, tokens))
        .collect(),
    );
    let table: Element<'a, Message> = match page.entries.as_deref() {
        None => label(t!("settings.loading")).into(),
        Some([]) if page.filter.trim().is_empty() => {
            hint(t!("settings.host_keys.none"), tokens).into()
        }
        Some([]) => hint(t!("settings.host_keys.no_match"), tokens).into(),
        // The rows are one stop of the ring; the page moves the selection with the arrows
        // (RC-080-16).
        Some(entries) => focus::choice(
            scrollable(Column::with_children(
                entries
                    .iter()
                    .enumerate()
                    .map(|(index, stored)| entry_row(page, stored, index, tokens)),
            ))
            .height(Length::Fill),
            "settings.host_keys.entries",
            None,
            None,
        ),
    };
    let mut body = column![
        fit::prose(t!("settings.host_keys.help")).color(tokens.text_secondary),
        row![search]
            .extend(count.map(Element::from))
            .spacing(8)
            .align_y(Alignment::Center),
        container(header)
            .padding([0, 6])
            .style(move |_| theme::header(tokens)),
        container(table).height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill);
    if let Some(stored) = page.asking.and_then(|item| page.entry(item)) {
        body = body.push(question(stored));
    }
    body.push(buttons(page)).into()
}

/// One row: server, port, type, fingerprint and when it was trusted; a click selects it.
fn entry_row<'a>(
    page: &HostKeysPage,
    stored: &'a Stored,
    index: usize,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let selected = page.selected == Some(stored.item());
    let host = if stored.hashed() {
        t!("settings.host_keys.hashed").to_owned()
    } else {
        stored.host().to_owned()
    };
    let port = stored
        .port()
        .map(|port| port.to_string())
        .unwrap_or_default();
    let [server, port_share, kind_share, print, since] = PORTIONS;
    let cells = vec![
        fit::path_cell(host, server, stored.hashed(), tokens),
        fit::cell(port, port_share, false, tokens),
        fit::cell(kind_of(stored), kind_share, false, tokens),
        fit::cut(
            stored.fingerprint().to_owned(),
            theme::MONO_FONT,
            None,
            print,
            tokens,
        ),
        fit::cell(shown_time(stored.added()), since, true, tokens),
    ];
    let item = stored.item();
    let alternate = theme::alternate(index);
    mouse_area(
        container(fit::table_row(false, tokens, cells))
            .padding([0, 6])
            .width(Length::Fill)
            .style(move |_| theme::striped_row(tokens, selected, true, false, alternate)),
    )
    .on_press(page_message(HostKeysMsg::Select(item)))
    .into()
}

/// The type column: the key's algorithm, or that it is an FTPS certificate.
fn kind_of(stored: &Stored) -> String {
    match stored {
        Stored::Key(key) => key.algorithm.clone(),
        Stored::Certificate(_) => t!("settings.host_keys.certificate").to_owned(),
    }
}

/// `2026-10-18T09:15:00.000Z` as `2026-10-18 09:15` (UTC); anything else as stored.
pub fn shown_time(stored: &str) -> String {
    match stored.get(..16) {
        Some(minutes) if stored.as_bytes().get(10) == Some(&b'T') => minutes.replacen('T', " ", 1),
        _ => stored.to_owned(),
    }
}

/// The question before a removal: what goes, and that the next connection asks again.
fn question<'a>(stored: &Stored) -> Element<'a, Message> {
    let text = match stored {
        Stored::Key(_) => t!("settings.host_keys.confirm_key", server = stored.server()),
        Stored::Certificate(_) => t!(
            "settings.host_keys.confirm_certificate",
            server = stored.server()
        ),
    };
    layout::button_row(
        None,
        [
            layout::secondary_icon_button(
                Icon::Delete,
                t!("settings.host_keys.remove"),
                Some(page_message(HostKeysMsg::ConfirmRemove)),
            ),
            layout::secondary_button(
                t!("settings.host_keys.keep"),
                Some(page_message(HostKeysMsg::KeepIt)),
            ),
        ],
        [Element::from(fit::prose(text).color(theme::DANGER))],
    )
}

/// Import at the left, Remove of the selected entry; both wait while one runs.
fn buttons<'a>(page: &HostKeysPage) -> Element<'a, Message> {
    let idle = !page.busy && page.asking.is_none();
    let selected = page.selected.filter(|item| page.entry(*item).is_some());
    row![
        layout::secondary_icon_button(
            Icon::Import,
            t!("settings.host_keys.import"),
            idle.then(|| page_message(HostKeysMsg::Import)),
        ),
        layout::secondary_icon_button(
            Icon::Delete,
            t!("settings.host_keys.remove"),
            selected
                .filter(|_| idle)
                .map(|_| page_message(HostKeysMsg::Remove)),
        ),
    ]
    .spacing(layout::BUTTON_SPACING)
    .into()
}
