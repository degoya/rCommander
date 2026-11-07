//! The page of the server inventories (RC-100-39): the sources on the left, each a button that
//! fills the form; the form on the right - name, inventory plugin, address, one hidden field per
//! slot of the plugin - with the last sync and what the last preview or sync did. A stored token
//! is never shown, only that one is stored.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, container, pick_list, row, scrollable},
};
use rc_engine::inventory::InventoryReport;
use rc_vault::SecretString;

use super::{Actions, field, hint, label};
use crate::{
    app::{
        Message,
        connections::inventory::{InventoryMsg, InventoryPage, msg},
    },
    i18n::{error_text, t},
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The width of the labels in front of the fields.
const LABEL_WIDTH: f32 = 150.0;
/// The plugin choice as a stop of the focus ring.
const PLUGIN_STOP_ID: &str = "rcommander.connections.inventory.plugin";

/// An inventory plugin in the list: its id and what it is shown as.
#[derive(Debug, Clone, PartialEq)]
struct Plugin(String, String);

impl fmt::Display for Plugin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.1)
    }
}

pub(super) fn page<'a>(page: &'a InventoryPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut list = Column::new().spacing(4);
    if page.sources.is_empty() {
        list = list.push(hint(t!("inventory.none"), tokens));
    }
    for source in &page.sources {
        let name: &'a str = source.name.as_str();
        list = list.push(layout::secondary_button(
            name,
            Some(msg(InventoryMsg::Pick(source.id))),
        ));
    }
    list = list.push(layout::secondary_icon_button(
        Icon::Add,
        t!("inventory.new"),
        Some(msg(InventoryMsg::New)),
    ));
    let side = column![
        label(t!("inventory.title")).font(theme::UI_FONT_STRONG),
        hint(t!("inventory.help"), tokens),
        scrollable(list).height(Length::Fill),
    ]
    .spacing(10);
    row![
        container(side)
            .width(Length::FillPortion(2))
            .height(Length::Fill),
        container(scrollable(form(page, tokens)))
            .width(Length::FillPortion(3))
            .height(Length::Fill),
    ]
    .spacing(16)
    .height(Length::Fill)
    .into()
}

/// One labelled line of the form.
fn line<'a>(title: &'a str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![label(title).width(LABEL_WIDTH), control.into()]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

fn form<'a>(page: &'a InventoryPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut lines = Column::new().spacing(8);
    if page.plugins.is_empty() {
        lines = lines.push(hint(t!("inventory.no_plugin"), tokens));
    }
    lines = lines.push(line(
        t!("inventory.name"),
        field(t!("inventory.name"), &page.name).on_input(|name| msg(InventoryMsg::Name(name))),
    ));
    let options: Vec<Plugin> = page
        .plugins
        .iter()
        .map(|plugin| {
            Plugin(
                plugin.id.clone(),
                format!("{} {}", plugin.name, plugin.version),
            )
        })
        .collect();
    let selected = options
        .iter()
        .find(|option| page.plugin.as_deref() == Some(option.0.as_str()))
        .cloned();
    let pick = |picked: Plugin| msg(InventoryMsg::Plugin(picked.0));
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(260);
    let choice: Element<'a, Message> = focus::Stop::pick(list, PLUGIN_STOP_ID, moves)
        .id(PLUGIN_STOP_ID)
        .into();
    lines = lines.push(line(t!("inventory.plugin"), choice));
    lines = lines.push(line(
        t!("inventory.address"),
        field(t!("inventory.address_hint"), &page.address)
            .on_input(|address| msg(InventoryMsg::Address(address))),
    ));
    let stored = page
        .current()
        .map(|source| source.slots.as_slice())
        .unwrap_or_default();
    for slot in page.slots() {
        let placeholder = if stored.contains(slot) {
            t!("connections.secret_stored")
        } else {
            t!("connections.secret_none")
        };
        let name = slot.clone();
        lines = lines.push(line(
            slot.as_str(),
            field(placeholder, page.typed(slot))
                .secure(true)
                .on_input(move |typed| {
                    msg(InventoryMsg::Typed(name.clone(), SecretString::from(typed)))
                }),
        ));
    }
    lines = lines.push(hint(t!("inventory.token_hint"), tokens));
    if let Some(source) = page.current() {
        let last = match (&source.last_sync_at, &source.last_outcome) {
            (None, _) => t!("inventory.never_synced").to_owned(),
            (Some(at), Some(outcome)) if outcome != "ok" => t!(
                "inventory.last_failed",
                at = at,
                error = error_text(outcome)
            ),
            (Some(at), _) => t!("inventory.last_sync", at = at),
        };
        lines = lines.push(fit::prose(last));
    }
    if let Some(report) = &page.report {
        lines = lines.push(report_view(report));
    }
    if let Some(error) = &page.error {
        lines = lines.push(fit::prose(error.as_str()).color(theme::DANGER));
    }
    lines.into()
}

/// What the last preview or sync did, in one paragraph.
fn report_view<'a>(report: &InventoryReport) -> Element<'a, Message> {
    let key_counts = t!(
        "inventory.report",
        created = report.created,
        updated = report.updated,
        unchanged = report.unchanged,
        missing = report.missing + report.still_missing,
        skipped = report.skipped + report.refused
    );
    let head = if report.preview {
        t!("inventory.report_preview")
    } else {
        t!("inventory.report_synced")
    };
    let mut lines = Column::new()
        .spacing(4)
        .push(label(head).font(theme::UI_FONT_STRONG))
        .push(fit::prose(key_counts));
    for (name, code) in &report.refusals {
        lines = lines.push(fit::prose(format!("{name}: {}", error_text(code))));
    }
    lines.into()
}

/// Back, Delete, Preview, Sync and Save, beside Close in the card's row of buttons.
pub(super) fn actions<'a>(page: &InventoryPage) -> Actions<'a> {
    let chosen = page.editing.is_some() && !page.busy;
    Actions {
        left: vec![layout::secondary_button(
            t!("inventory.delete"),
            chosen.then(|| msg(InventoryMsg::Delete)),
        )],
        right: vec![
            layout::secondary_button(t!("inventory.back"), Some(msg(InventoryMsg::Back))),
            layout::secondary_button(
                t!("inventory.preview"),
                chosen.then(|| msg(InventoryMsg::Preview)),
            ),
            layout::secondary_icon_button(
                Icon::Import,
                t!("inventory.sync"),
                chosen.then(|| msg(InventoryMsg::Sync)),
            ),
            layout::primary_button(
                t!("inventory.save"),
                page.can_save().then(|| msg(InventoryMsg::Save)),
            ),
        ],
    }
}
