//! Under the form of a stored connection: for each secret it uses, "from" the vault or a secret
//! source, the entry it is mapped to, and the search that maps it (`connections/sources.rs`).

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, button, pick_list, row},
};
use rc_engine::SecretKind;

use super::{field, hint, label, wrap};
use crate::{
    app::{
        Message,
        connections::{
            ConnMsg, ConnectionManager,
            form::Form,
            sources::{SOURCE_QUERY_FIELD, SourceMsg, SourceSearch},
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The width of the labels in front of the choices.
const LABEL_WIDTH: f32 = 150.0;

/// A choice of the list: the vault, or a source by its name and state.
#[derive(Debug, Clone, PartialEq)]
struct From(Option<String>, String);

impl fmt::Display for From {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.1)
    }
}

fn source(message: SourceMsg) -> Message {
    wrap(ConnMsg::Source(message))
}

/// The panel; nothing while no source is installed.
pub(super) fn panel<'a>(
    manager: &'a ConnectionManager,
    form: &'a Form,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let panel = &manager.sources;
    if panel.sources.is_empty() {
        return None;
    }
    let mut kinds = Vec::new();
    if form.uses_password() {
        kinds.push(SecretKind::Password);
    }
    if form.uses_key() {
        kinds.extend([SecretKind::Key, SecretKind::Passphrase]);
    }
    if kinds.is_empty() {
        return None;
    }
    let mut options = vec![From(None, t!("connections.source.vault").to_owned())];
    options.extend(panel.sources.iter().map(|info| {
        From(
            Some(info.source.clone()),
            t!(
                "connections.source.option",
                name = info.name,
                state = state_label(&info.state)
            ),
        )
    }));
    let mut lines = Column::new()
        .spacing(6)
        .push(label(t!("connections.source.title")).font(theme::UI_FONT_STRONG));
    for kind in kinds {
        let mapped = panel.mapping(kind);
        let searching = panel
            .search
            .as_ref()
            .filter(|search| search.kind == Some(kind));
        let current = searching
            .map(|search| Some(search.source.clone()))
            .unwrap_or_else(|| mapped.map(|mapping| mapping.source.clone()));
        let selected = options.iter().find(|option| option.0 == current).cloned();
        let id = choice_id(kind);
        let pick = move |picked: From| source(SourceMsg::From(kind, picked.0));
        let moves = focus::moves(&options, selected.as_ref(), &pick);
        // Each kind's list holds its own copy: one list for up to three rows.
        let list = pick_list(options.clone(), selected, pick)
            .text_size(theme::TEXT_SIZE)
            .width(260);
        let choice = focus::Stop::pick(list, id, moves).id(id);
        lines = lines.push(
            row![label(kind_label(kind)).width(LABEL_WIDTH), choice,]
                .spacing(8)
                .align_y(Alignment::Center),
        );
        if let Some(mapping) = mapped.filter(|_| searching.is_none()) {
            let entry = t!("connections.source.entry", label = mapping.entry_label);
            lines = lines.push(indented(fit::cut(entry, theme::UI_FONT, None, 1, tokens)));
        }
        if let Some(search) = searching {
            lines = lines.push(search_block(search, tokens));
        }
    }
    Some(lines.into())
}

/// The search field, its button, an error or the entries found, each a button that maps it.
fn search_block<'a>(search: &'a SourceSearch, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut block = Column::new().spacing(4).push(
        row![
            field(t!("connections.source.query"), &search.query)
                .id(SOURCE_QUERY_FIELD)
                .on_input(|text| source(SourceMsg::Query(text)))
                .on_submit(source(SourceMsg::Search)),
            layout::secondary_icon_button(
                Icon::Search,
                t!("connections.source.search"),
                Some(source(SourceMsg::Search)),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    );
    if let Some(error) = &search.error {
        block = block.push(fit::prose(error.as_str()).color(theme::DANGER));
    } else if search.searched && search.entries.is_empty() {
        block = block.push(hint(t!("connections.source.none_found").to_owned(), tokens));
    }
    for (at, entry) in search.entries.iter().enumerate() {
        let mut text = entry.title.clone();
        for part in [&entry.user, &entry.url].into_iter().flatten() {
            text.push_str(" \u{00b7} ");
            text.push_str(part);
        }
        let pick = source(SourceMsg::Pick(at));
        block = block.push(focus::button(
            button(fit::cut(text, theme::UI_FONT, None, 1, tokens))
                .width(Length::Fill)
                .padding([4, 10])
                .style(button::secondary)
                .on_press(pick.clone()),
            &entry.id,
            Some(pick),
        ));
    }
    indented(block.into())
}

fn indented(content: Element<'_, Message>) -> Element<'_, Message> {
    row![Space::new().width(LABEL_WIDTH + 8.0), content].into()
}

fn choice_id(kind: SecretKind) -> &'static str {
    match kind {
        SecretKind::Password => "rcommander.connections.source_password",
        SecretKind::Key => "rcommander.connections.source_key",
        SecretKind::Passphrase => "rcommander.connections.source_passphrase",
    }
}

fn kind_label(kind: SecretKind) -> &'static str {
    match kind {
        SecretKind::Password => t!("connections.source.password"),
        SecretKind::Key => t!("connections.source.key"),
        SecretKind::Passphrase => t!("connections.source.passphrase"),
    }
}

fn state_label(state: &str) -> &'static str {
    match state {
        "ready" => t!("connections.source.state.ready"),
        "locked" => t!("connections.source.state.locked"),
        "unreachable" => t!("connections.source.state.unreachable"),
        "unlicensed" => t!("connections.source.state.unlicensed"),
        _ => t!("connections.source.state.failed"),
    }
}
