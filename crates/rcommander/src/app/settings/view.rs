//! Drawing the dialog in its window: the page list, the page - under its tabs where it has them
//! (`tabs.rs`) - the buttons and the last note. A schema page draws its rows by their type, so a
//! new setting needs no code here.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{
        Column, button, checkbox, column, container, pick_list, radio, row, scrollable, text_input,
    },
};
use rc_engine::{
    SETTINGS,
    settings::{SettingKind, SettingPage, SettingSpec},
};

use super::{
    Page, SettingsDialog, SettingsMessage,
    draft::{Draft, Field, Limit},
    fonts_page, host_keys_view, mcp_page, plugins_view, shortcuts_view,
    tabs::{self, Tab, Tabs},
    themes_page, wrap,
};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        connections::{ConnMsg, TcImportMsg},
        database_dialog::{DatabaseMsg, DialogMode},
    },
    i18n::{lookup, t},
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

pub(super) use crate::theme::fit::{hint, label};

impl SettingsDialog {
    /// The dialog's content - page list, page, buttons - without its title: the window's title
    /// bar carries it (`app/dialog_windows.rs`). `shown` is the tab each page shows.
    pub(in crate::app) fn view(
        &self,
        shown: Tabs,
        tokens: &'static Tokens,
    ) -> Element<'_, Message> {
        let pages = Page::ALL.into_iter().map(|page| {
            let chosen = page == self.page;
            button(label(page_title(page)))
                .width(Length::Fill)
                .padding([6, 10])
                .style(move |_, status| theme::page_entry(tokens, chosen, status))
                .on_press(wrap(SettingsMessage::Page(page)))
                .into()
        });
        // The page list is one stop of the ring; the arrows choose the page (RC-080-16).
        let pages = focus::pick(
            Column::with_children(pages).spacing(2).width(170),
            "settings.pages",
            &Page::ALL,
            Some(&self.page),
            |page| wrap(SettingsMessage::Page(page)),
        );
        let tab = shown.chosen(self.page);
        let body: Element<'_, Message> = match &self.draft {
            _ if self.page == Page::About => super::about::view(&self.about, tokens),
            _ if self.page == Page::HostKeys => host_keys_view::view(&self.host_keys, tokens),
            Some(draft) if tab == Some(Tab::Shortcuts) => {
                shortcuts_view::view(&self.shortcuts, draft, tokens)
            }
            // The themes and their editor, then the colours by file type (RC-100-50).
            Some(draft) if tab == Some(Tab::Themes) => scrollable(
                container(themes_page::view(&self.themes, draft, tokens)).padding([0, 12]),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            Some(_) if tab == Some(Tab::Installed) => {
                scrollable(plugins_view::view(&self.plugins, tokens).padding([0, 12]))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            // The self-update's state over its switches (RC-100-05).
            Some(draft) if self.page == Page::Updates => scrollable(
                column![
                    crate::app::update_ui::page(self.updates.as_deref(), tokens),
                    page_view(self.page, tab, draft, tokens),
                ]
                .spacing(16)
                .padding([0, 12]),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            Some(draft) => scrollable(page_view(self.page, tab, draft, tokens).padding([0, 12]))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => label(t!("settings.loading")).into(),
        };
        // The tabs stand over the page and stay while it scrolls.
        let body: Element<'_, Message> = match tab {
            Some(chosen) => column![tabs::strip(self.page, chosen, tokens), body]
                .spacing(12)
                .height(Length::Fill)
                .into(),
            None => body,
        };
        // The last note stands at the left of the row and wraps.
        let note = self.note.as_ref().map(|(note, error)| {
            let color = if *error {
                theme::DANGER
            } else {
                tokens.text_secondary
            };
            Element::from(fit::prose(note.as_str()).color(color))
        });
        let ready = self.draft.is_some() && !self.busy;
        let on = |message: SettingsMessage| ready.then(|| wrap(message));
        let buttons = layout::button_row(
            Some(layout::primary_icon_button(
                Icon::Confirm,
                t!("settings.ok"),
                on(SettingsMessage::Ok),
            )),
            [
                layout::secondary_icon_button(
                    Icon::Close,
                    t!("settings.cancel"),
                    Some(wrap(SettingsMessage::Cancel)),
                ),
                layout::secondary_button(t!("settings.apply"), on(SettingsMessage::Apply)),
            ],
            note,
        );
        column![
            row![
                pages,
                container(body).width(Length::Fill).height(Length::Fill),
            ]
            .spacing(12)
            .height(Length::Fill),
            buttons,
        ]
        .spacing(12)
        .into()
    }
}

/// The text of a page's entry in the list.
fn page_title(page: Page) -> &'static str {
    match page {
        Page::Appearance => t!("settings.page.appearance"),
        Page::Folders => t!("settings.page.folders"),
        Page::Keyboard => t!("settings.page.keyboard"),
        Page::Terminal => t!("settings.page.terminal"),
        Page::Vault => t!("settings.page.vault"),
        Page::HostKeys => t!("settings.page.host_keys"),
        Page::Retention => t!("settings.page.retention"),
        Page::Mcp => t!("settings.page.mcp"),
        Page::Programs => t!("settings.page.programs"),
        Page::Transfers => t!("settings.page.transfers"),
        Page::Plugins => t!("settings.page.plugins"),
        Page::Import => t!("settings.page.import"),
        Page::Updates => t!("settings.page.updates"),
        Page::About => t!("settings.page.about"),
    }
}

/// The schema page a dialog page draws; none for retention.
fn schema_page(page: Page) -> Option<SettingPage> {
    match page {
        Page::Appearance => Some(SettingPage::Appearance),
        Page::Folders => Some(SettingPage::Folders),
        Page::Keyboard => Some(SettingPage::Keyboard),
        Page::Terminal => Some(SettingPage::Terminal),
        Page::Vault => Some(SettingPage::Vault),
        Page::Mcp => Some(SettingPage::Mcp),
        Page::Import => Some(SettingPage::Exchange),
        Page::Retention | Page::HostKeys | Page::About => None,
        Page::Programs => Some(SettingPage::Programs),
        Page::Transfers => Some(SettingPage::Transfers),
        Page::Plugins => Some(SettingPage::Plugins),
        Page::Updates => Some(SettingPage::Updates),
    }
}

/// The dialog page every schema page is drawn on: each setting appears on exactly one. The
/// tests hold every schema page against the dialog's.
#[cfg(test)]
pub(super) fn dialog_page(page: SettingPage) -> Page {
    match page {
        SettingPage::Appearance => Page::Appearance,
        SettingPage::Folders => Page::Folders,
        SettingPage::Keyboard => Page::Keyboard,
        SettingPage::Terminal => Page::Terminal,
        SettingPage::Vault => Page::Vault,
        SettingPage::Mcp => Page::Mcp,
        SettingPage::Programs => Page::Programs,
        SettingPage::Transfers => Page::Transfers,
        SettingPage::Exchange => Page::Import,
        SettingPage::Plugins => Page::Plugins,
        SettingPage::Updates => Page::Updates,
    }
}

/// The rows of `page` under its tab `tab` (`None`: a page without tabs, every row).
pub(super) fn page_view<'a>(
    page: Page,
    tab: Option<Tab>,
    draft: &'a Draft,
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let mut rows = Column::new().spacing(16);
    if let Some(schema) = schema_page(page) {
        for spec in SETTINGS
            .iter()
            .filter(|spec| spec.page == schema && tabs::shows(tab, spec))
        {
            rows = rows.push(setting_row(spec, draft, tokens));
        }
    }
    match page {
        Page::Retention => rows = rows.push(retention(draft, tokens)),
        Page::Mcp => rows = rows.push(mcp_page::view(draft, tokens)),
        Page::Import => rows = rows.push(import(draft, tokens)),
        _ => {}
    }
    rows
}

/// The imports from other programs; Total Commander's opens its page of the connection manager.
/// The export and import of rCommander's own settings and connections (RC-080-19) open the
/// database dialog in the main window, which takes the settings' place: offered only while
/// nothing here is unsaved.
fn import<'a>(draft: &Draft, tokens: &'static Tokens) -> Element<'a, Message> {
    let open = Message::Connections(ConnMsg::TcImport(TcImportMsg::Open));
    let saved = draft.changes().is_ok_and(|changes| changes.is_empty());
    let exchange = |mode| saved.then_some(Message::Database(DatabaseMsg::OpenMode(mode)));
    column![
        label(t!("settings.import.total_commander")),
        hint(t!("settings.import.total_commander_help"), tokens),
        layout::secondary_icon_button(Icon::Import, t!("tc_import.open"), Some(open)),
        label(t!("settings.import.rcommander")),
        hint(t!("settings.import.rcommander_help"), tokens),
        row![
            layout::secondary_button(t!("settings.import.export"), exchange(DialogMode::Export)),
            layout::secondary_icon_button(
                Icon::Import,
                t!("settings.import.import"),
                exchange(DialogMode::Import)
            ),
        ]
        .spacing(8),
    ]
    .spacing(6)
    .into()
}

/// One setting: its label, its control by type, its help and whether it waits for a restart.
fn setting_row<'a>(
    spec: &'static SettingSpec,
    draft: &'a Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let key = spec.key;
    let text = text_of(draft, key);
    let control: Element<'a, Message> = match spec.kind {
        SettingKind::Bool { .. } => {
            let on = matches!(draft.field(key), Some(Field::Bool(true)));
            let check = checkbox(on)
                .label(lookup(spec.label))
                .on_toggle(move |on| wrap(SettingsMessage::Toggle(key, on)));
            focus::toggle(check, key, Some(wrap(SettingsMessage::Toggle(key, !on))))
        }
        SettingKind::Choice { choices, .. } => {
            let current = choices
                .iter()
                .find(|choice| choice.value == text)
                .map(|choice| choice.value);
            let radios = choices.iter().map(|choice| {
                radio(lookup(choice.label), choice.value, current, move |value| {
                    wrap(SettingsMessage::Edit(key, value.to_owned()))
                })
                .into()
            });
            // The group is one stop; the arrows choose, as in Windows.
            let values: Vec<&'static str> = choices.iter().map(|choice| choice.value).collect();
            focus::pick(
                Column::with_children(radios).spacing(6),
                key,
                &values,
                current.as_ref(),
                move |value| wrap(SettingsMessage::Edit(key, value.to_owned())),
            )
        }
        SettingKind::Id { .. } => {
            let mut options = vec![ShellChoice(String::new())];
            options.extend(draft.shells().iter().cloned().map(ShellChoice));
            let chosen = ShellChoice(text.to_owned());
            let edit = move |choice: ShellChoice| wrap(SettingsMessage::Edit(key, choice.0));
            let moves = focus::moves(&options, Some(&chosen), &edit);
            let list = pick_list(options, Some(chosen), edit).width(240);
            focus::Stop::pick(list, key, moves).into()
        }
        SettingKind::Int { .. } => text_input("", text)
            .on_input(move |text| wrap(SettingsMessage::Edit(key, text)))
            .width(120)
            .into(),
        SettingKind::Path { .. } => row![
            text_input(path_placeholder(spec.page), text)
                .on_input(move |text| wrap(SettingsMessage::Edit(key, text)))
                .width(Length::Fill),
            browse::button(PathField::Setting(spec), text, true),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        SettingKind::Font { .. } => fonts_page::control(key, draft, tokens),
        SettingKind::ColourRules { .. } => super::colours_page::rules(draft, tokens),
        // The keymap's deviations are the Shortcuts tab's table (`tabs::shows`), never drawn here;
        // the themes and RC-100-43's colours are the Themes tab's (`themes_page.rs`).
        SettingKind::Colour { .. }
        | SettingKind::Theme { .. }
        | SettingKind::Extensions { .. }
        | SettingKind::Associations { .. }
        | SettingKind::Source { .. }
        | SettingKind::Proxy { .. }
        | SettingKind::Keymap { .. } => text_input("", text)
            .on_input(move |text| wrap(SettingsMessage::Edit(key, text)))
            .width(240)
            .into(),
    };
    let mut rows = Column::new().spacing(4);
    if !matches!(spec.kind, SettingKind::Bool { .. }) {
        rows = rows.push(label(lookup(spec.label)));
    }
    rows = rows.push(control).push(hint(lookup(spec.help), tokens));
    if spec.restart {
        rows = rows.push(hint(t!("settings.restart_note"), tokens));
    }
    rows.into()
}

/// What an empty path field shows: the home folder, or for a program that none is set.
fn path_placeholder(page: SettingPage) -> &'static str {
    match page {
        SettingPage::Programs => t!("settings.not_set"),
        _ => t!("settings.home_folder"),
    }
}

/// The text of a field; empty for a switch.
fn text_of<'a>(draft: &'a Draft, key: &str) -> &'a str {
    match draft.field(key) {
        Some(Field::Text(text)) => text.as_str(),
        _ => "",
    }
}

/// The limits of each table that grows with use.
fn retention<'a>(draft: &'a Draft, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut rows = Column::new()
        .spacing(10)
        .push(label(t!("settings.retention.title")))
        .push(hint(t!("settings.retention.help"), tokens));
    for (index, table) in draft.retention.iter().enumerate() {
        let status = &table.status;
        let field = |value: &str, limit: Limit| {
            text_input("", value)
                .on_input(move |text| wrap(SettingsMessage::Retention(index, limit, text)))
                .width(100)
        };
        rows = rows.push(
            column![
                label(status.table.as_str()),
                row![
                    label(t!("settings.retention.rows")).width(120),
                    field(&table.rows, Limit::Rows),
                    hint(
                        t!(
                            "settings.retention.bounds",
                            min = status.floor.max_rows,
                            max = status.ceiling.max_rows
                        ),
                        tokens
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    label(t!("settings.retention.days")).width(120),
                    field(&table.days, Limit::Days),
                    hint(
                        t!(
                            "settings.retention.bounds",
                            min = status.floor.max_age_days,
                            max = status.ceiling.max_age_days
                        ),
                        tokens
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(6),
        );
    }
    rows.into()
}

/// A shell id in the list, the empty one for "the first shell found".
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellChoice(String);

impl fmt::Display for ShellChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            formatter.write_str(t!("settings.shell_auto"))
        } else {
            formatter.write_str(&self.0)
        }
    }
}
