//! Drawing the Shortcuts tab: the help line, the search, the table of keys - one stop of the
//! focus ring -, the recording's prompt or the question before a key moves, and the tab's
//! buttons.

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, container, mouse_area, row, scrollable, text_input},
};
use rc_engine::{KeyContext, KeymapRow};

use super::{
    draft::Draft,
    shortcuts::{
        Recording, Selected, ShortcutsMsg, ShortcutsPage, keymap_of, label, lock_reason,
        page_message, row_of, shown,
    },
    view::hint,
};
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The columns' shares of the width: action, key, where it applies, the mark.
const PORTIONS: [u16; 4] = [6, 3, 2, 2];

pub fn view<'a>(
    page: &'a ShortcutsPage,
    draft: &Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let keymap = keymap_of(draft);
    let rows = shown(&keymap, &page.search);
    let find = t!("settings.shortcuts.find");
    let search = text_input(t!("settings.shortcuts.search"), &page.search)
        .on_input(|search| page_message(ShortcutsMsg::Search(search)))
        .padding([5, 8])
        .size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .width(240);
    let search = layout::keyed(focus::Stop::field(search, find), find);
    let count = hint(t!("settings.shortcuts.count", count = rows.len()), tokens);
    let header = fit::table_row(
        false,
        tokens,
        [
            t!("settings.shortcuts.action"),
            t!("settings.shortcuts.key"),
            t!("settings.shortcuts.when"),
            t!("settings.shortcuts.mark"),
        ]
        .into_iter()
        .zip(PORTIONS)
        .map(|(title, portion)| fit::cell(title, portion, true, tokens))
        .collect(),
    );
    let table: Element<'a, Message> = if rows.is_empty() {
        hint(t!("settings.shortcuts.no_match"), tokens).into()
    } else {
        // The rows are one stop of the ring; the tab moves the selection with the arrows.
        focus::choice(
            scrollable(Column::with_children(
                rows.iter()
                    .enumerate()
                    .map(|(index, row)| key_row(page, row, index, tokens)),
            ))
            .height(Length::Fill),
            "settings.shortcuts.keys",
            None,
            None,
        )
    };
    let selected = page.selected.as_ref().and_then(|s| row_of(&rows, s));
    let mut body = column![
        fit::prose(t!("settings.shortcuts.help")).color(tokens.text_secondary),
        row![layout::caption(find, None), search, count]
            .spacing(8)
            .align_y(Alignment::Center),
        container(header)
            .padding([0, 6])
            .style(move |_| theme::header(tokens)),
        container(table).height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill);
    if let (Some(_), Some(row)) = (page.recording, selected) {
        let prompt = t!("settings.shortcuts.recording", name = label(&row.name));
        body = body.push(fit::prose(prompt).color(tokens.accent));
    }
    if let (Some(conflict), Some(row)) = (&page.asking, selected) {
        body = body.push(question(
            &conflict.chord.to_string(),
            &conflict.holder,
            &row.name,
        ));
    }
    body.push(buttons(page, selected)).into()
}

/// One row: the action, its key - the reason in a tooltip when the window keeps it -, where it
/// applies and whether it was changed; a click selects it.
fn key_row<'a>(
    page: &ShortcutsPage,
    row: &KeymapRow,
    index: usize,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let this = Selected {
        name: row.name.clone(),
        keys: row.keys.clone(),
    };
    let chosen = page.selected.as_ref() == Some(&this);
    let [action, key_share, when_share, mark_share] = PORTIONS;
    let keys = if row.keys.is_empty() {
        t!("settings.shortcuts.no_key").to_owned()
    } else {
        row.keys.clone()
    };
    let key_cell = fit::cell(keys, key_share, row.keys.is_empty(), tokens);
    let when = match row.when {
        KeyContext::Files => t!("settings.shortcuts.when.files"),
        KeyContext::Always => t!("settings.shortcuts.when.always"),
    };
    let mark = match (row.locked, row.changed) {
        (Some(_), _) => t!("settings.shortcuts.locked"),
        (None, true) => t!("settings.shortcuts.changed"),
        (None, false) => "",
    };
    let mark_cell = fit::cell(mark, mark_share, row.locked.is_some(), tokens);
    let cells = vec![
        fit::cell(label(&row.name), action, false, tokens),
        key_cell,
        fit::cell(when, when_share, true, tokens),
        mark_cell,
    ];
    let line = container(fit::table_row(false, tokens, cells))
        .padding([0, 6])
        .width(Length::Fill)
        .style(move |_| theme::striped_row(tokens, chosen, true, false, theme::alternate(index)));
    let line: Element<'a, Message> = match row.locked {
        Some(lock) => fit::tip(line, lock_reason(lock).to_owned(), tokens),
        None => line.into(),
    };
    mouse_area(line)
        .on_press(page_message(ShortcutsMsg::Select(this)))
        .into()
}

/// The question before a key moves: which row has it now and which gets it.
fn question<'a>(key: &str, holder: &str, name: &str) -> Element<'a, Message> {
    let text = t!(
        "settings.shortcuts.conflict",
        key = key,
        holder = label(holder),
        name = label(name)
    );
    layout::button_row(
        None,
        [
            layout::secondary_button(
                t!("settings.shortcuts.replace"),
                Some(page_message(ShortcutsMsg::Replace)),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.keep"),
                Some(page_message(ShortcutsMsg::KeepIt)),
            ),
        ],
        [Element::from(fit::prose(text).color(theme::DANGER))],
    )
}

/// The selected row's buttons, then "Reset all"; they wait while a key is recorded or the
/// question is open. A row the window keeps gets no other key and keeps its own.
fn buttons<'a>(page: &ShortcutsPage, selected: Option<&KeymapRow>) -> Element<'a, Message> {
    let idle = page.recording.is_none() && page.asking.is_none();
    let row_of_action = selected.filter(|row| idle && row.command.is_some());
    let free = row_of_action.filter(|row| row.locked.is_none());
    let on = |row: Option<&KeymapRow>, message: ShortcutsMsg| row.map(|_| page_message(message));
    let with_key = free.filter(|row| !row.keys.is_empty());
    column![
        row![
            layout::secondary_button(
                t!("settings.shortcuts.change"),
                on(free, ShortcutsMsg::Record(Recording::Change)),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.add"),
                on(row_of_action, ShortcutsMsg::Record(Recording::Add)),
            ),
            layout::secondary_icon_button(
                Icon::Delete,
                t!("settings.shortcuts.remove"),
                on(with_key, ShortcutsMsg::Remove),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
        row![
            layout::secondary_button(
                t!("settings.shortcuts.reset"),
                on(row_of_action, ShortcutsMsg::Reset),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.reset_all"),
                idle.then(|| page_message(ShortcutsMsg::ResetAll)),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
    ]
    .spacing(6)
    .into()
}
