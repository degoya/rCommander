//! Drawing the button bars' window (RC-100-87): the bar to edit with New, Delete and "Import
//! .bar..." over its name; the items on the left with their buttons, the form of the item at the
//! cursor on the right; or, while an import is open, the `.bar` file's preview; the note and the
//! one row of buttons below; or, while "Choose symbol..." is open, the symbol picker's page
//! (`app/symbol_picker`, RC-110-10). A large dialog: in a window of its own while
//! `appearance.dialog_windows` is on, else a card over the main window.

use iced::{
    Alignment, Element, Length,
    widget::{Column, Row, Space, column, container, mouse_area, row, scrollable, text},
};
use rc_engine::{
    BarButton, BarItem, ButtonAction, ButtonBar, ButtonBarId,
    button_bars::{ImportPreview, PreviewResult},
};

use super::{BarWindow, Does, Import, WinMsg, win};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        symbol_picker,
        user_commands::{Labeled, choice, command_fields, field, help, labeled},
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The bar's name: the keyboard starts here.
pub const NAME_FIELD: &str = "rcommander.button_bar.name";
/// The items' list as one stop of the focus ring.
pub const LIST_STOP_ID: &str = "rcommander.button_bar.items";

/// The window over the main window, as a card.
pub fn layer<'a>(
    window: &'a BarWindow,
    bars: &'a [ButtonBar],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    theme::large_dialog(
        Message::dialog_head(fit::label(t!("button_bar.title")).size(18)),
        body(window, bars, tokens),
        tokens,
    )
}

/// The bar's row, the page and the one row of buttons.
pub fn body<'a>(
    window: &'a BarWindow,
    bars: &'a [ButtonBar],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    // "Choose symbol..." takes the page while it is open (RC-110-10).
    if let Some(picker) = &window.picker {
        return symbol_picker::view::page(picker, super::symbol::picker_message, tokens);
    }
    let mut options: Vec<Labeled<Option<ButtonBarId>>> = bars
        .iter()
        .map(|bar| Labeled(Some(bar.id), bar.name.clone()))
        .collect();
    if window.bar.is_none() {
        options.push(Labeled(None, t!("button_bar.new_bar").to_owned()));
    }
    let chosen = choice(
        options,
        &window.bar,
        |id| match id {
            Some(id) => win(WinMsg::Bar(id)),
            None => win(WinMsg::NewBar),
        },
        "button_bar.bar",
    );
    let head = Row::new()
        .push(labeled(t!("button_bar.field.bar"), chosen))
        .push(layout::secondary_icon_button(
            Icon::Add,
            t!("button_bar.new"),
            Some(win(WinMsg::NewBar)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Delete,
            t!("button_bar.delete_bar"),
            window.bar.map(|_| win(WinMsg::DeleteBar)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Import,
            t!("button_bar.import"),
            Some(win(WinMsg::ImportOpen)),
        ))
        .spacing(8)
        .align_y(Alignment::Center);
    let name = field("", &window.name)
        .id(NAME_FIELD)
        .on_input(|typed| win(WinMsg::Name(typed)))
        .on_submit(win(WinMsg::Save));
    let mut top = column![head, labeled(t!("button_bar.field.name"), name)].spacing(8);
    if window.asking {
        top = top.push(
            row![
                fit::label(t!("button_bar.delete_question")).width(Length::Fill),
                layout::secondary_button(t!("button_bar.yes"), Some(win(WinMsg::Confirm(true)))),
                layout::secondary_button(t!("button_bar.no"), Some(win(WinMsg::Confirm(false)))),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    let main: Element<'a, Message> = match &window.import {
        Some(import) => import_page(import, tokens),
        None => row![
            container(items_side(window, bars, tokens))
                .width(Length::FillPortion(2))
                .height(Length::Fill),
            container(form_side(window, bars, tokens))
                .width(Length::FillPortion(3))
                .height(Length::Fill),
        ]
        .spacing(16)
        .height(Length::Fill)
        .into(),
    };
    let note: Element<'a, Message> = match &window.note {
        Some((note, error)) => {
            let color = if *error {
                theme::DANGER
            } else {
                tokens.text_secondary
            };
            fit::label(note.as_str())
                .color(color)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into()
        }
        None => Space::new().into(),
    };
    let stored = window.bar.is_some() && !window.dirty;
    let buttons = Row::new()
        .push(container(note).width(Length::Fill))
        .push(layout::secondary_button(
            t!("button_bar.use_horizontal"),
            stored.then(|| win(WinMsg::UseAs(rc_engine::BarPlace::Horizontal))),
        ))
        .push(layout::secondary_button(
            t!("button_bar.use_vertical"),
            stored.then(|| win(WinMsg::UseAs(rc_engine::BarPlace::Vertical))),
        ))
        .push(layout::primary_icon_button(
            Icon::Save,
            t!("button_bar.save"),
            (window.dirty && !window.busy).then(|| win(WinMsg::Save)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Close,
            t!("button_bar.close"),
            Some(win(WinMsg::Close)),
        ))
        .spacing(12)
        .align_y(Alignment::Center);
    column![top, main, buttons].spacing(12).into()
}

/// The words an item shows in the list.
fn item_words(item: &BarItem, bars: &[ButtonBar]) -> String {
    match item {
        BarItem::Separator => t!("button_bar.separator").to_owned(),
        BarItem::Button(button) => button_words(button, bars),
    }
}

/// A button's tooltip, else what it runs or opens.
fn button_words(button: &BarButton, bars: &[ButtonBar]) -> String {
    if !button.tooltip.is_empty() {
        return button.tooltip.clone();
    }
    match &button.action {
        ButtonAction::Command(action) if action.command.is_empty() => {
            t!("button_bar.no_command").to_owned()
        }
        ButtonAction::Command(action) => action.command.clone(),
        ButtonAction::SubBar(id) => {
            let name = bars
                .iter()
                .find(|bar| bar.id == *id)
                .map_or_else(|| id.to_string(), |bar| bar.name.clone());
            t!("button_bar.opens", name = name)
        }
    }
}

/// The items, one stop of the ring, and their buttons.
fn items_side<'a>(
    window: &'a BarWindow,
    bars: &'a [ButtonBar],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut lines = Column::new().spacing(1);
    if window.items.is_empty() {
        lines = lines.push(fit::hint(t!("button_bar.empty"), tokens));
    }
    for (at, item) in window.items.iter().enumerate() {
        let cursor = window.cursor == Some(at);
        let shown = container(fit::cut(
            item_words(item, bars),
            theme::UI_FONT,
            None,
            1,
            tokens,
        ))
        .padding([0, 6])
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, true, false, theme::alternate(at)));
        // focus-ring: pointer only (a row of the list, which is one stop of the ring)
        lines = lines.push(mouse_area(shown).on_press(win(WinMsg::Select(at))));
    }
    let list = container(scrollable(lines).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, true));
    let list = focus::Stop::choice(list, LIST_STOP_ID, None, None).id(LIST_STOP_ID);
    let chosen = window.cursor.is_some();
    let buttons = Row::new()
        .push(layout::secondary_icon_button(
            Icon::Add,
            t!("button_bar.add_button"),
            Some(win(WinMsg::AddButton)),
        ))
        .push(layout::secondary_button(
            t!("button_bar.add_separator"),
            Some(win(WinMsg::AddSeparator)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Delete,
            t!("button_bar.remove"),
            chosen.then(|| win(WinMsg::Remove)),
        ))
        .push(layout::glyph_button(
            Icon::MoveUp,
            t!("button_bar.move_up"),
            chosen.then(|| win(WinMsg::Shift(-1))),
            tokens,
        ))
        .push(layout::glyph_button(
            Icon::MoveDown,
            t!("button_bar.move_down"),
            chosen.then(|| win(WinMsg::Shift(1))),
            tokens,
        ))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
    column![
        Element::from(list),
        buttons,
        fit::help_line(t!("button_bar.keys"), tokens)
    ]
    .spacing(8)
    .into()
}

/// The form of the item at the cursor: a button's command or sub-bar, symbol and tooltip.
fn form_side<'a>(
    window: &'a BarWindow,
    bars: &'a [ButtonBar],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let item = window.cursor.and_then(|at| window.items.get(at));
    let Some(BarItem::Button(button)) = item else {
        let words = if item.is_some() {
            t!("button_bar.separator_help")
        } else {
            t!("button_bar.choose")
        };
        return container(fit::hint(words, tokens)).padding(8).into();
    };
    let does = match button.action {
        ButtonAction::Command(_) => Does::Command,
        ButtonAction::SubBar(_) => Does::SubBar,
    };
    let kinds = vec![
        Labeled(Does::Command, t!("button_bar.does.command").to_owned()),
        Labeled(Does::SubBar, t!("button_bar.does.sub_bar").to_owned()),
    ];
    let mut rows = Column::new().spacing(8).push(labeled(
        t!("button_bar.field.does"),
        choice(
            kinds,
            &does,
            |does| win(WinMsg::Does(does)),
            "button_bar.does",
        ),
    ));
    match &button.action {
        ButtonAction::Command(action) => {
            rows = rows.push(command_fields(
                action,
                |change| win(WinMsg::Cmd(change)),
                tokens,
            ));
            if action.kind == rc_engine::CommandKind::Program
                && let Some(browse) =
                    browse::button(PathField::ButtonProgram, &action.command, true)
            {
                rows = rows.push(row![Space::new().width(148), browse]);
            }
        }
        ButtonAction::SubBar(id) => {
            let others: Vec<Labeled<ButtonBarId>> = bars
                .iter()
                .filter(|bar| Some(bar.id) != window.bar)
                .map(|bar| Labeled(bar.id, bar.name.clone()))
                .collect();
            let sub = choice(
                others,
                id,
                |id| win(WinMsg::SubBar(id)),
                "button_bar.sub_bar",
            );
            rows = rows
                .push(labeled(t!("button_bar.field.sub_bar"), sub))
                .push(help(t!("button_bar.help.sub_bar"), tokens));
        }
    }
    let icon_path = button.icon.as_ref().map_or("", |icon| icon.path.as_str());
    let icon_index = button
        .icon
        .as_ref()
        .map_or_else(String::new, |icon| icon.index.to_string());
    let mut symbol = Row::new()
        .push(
            field(t!("button_bar.icon_placeholder"), icon_path)
                .on_input(|typed| win(WinMsg::IconPath(typed))),
        )
        .push(
            field("0", &icon_index)
                .width(56)
                .on_input(|typed| win(WinMsg::IconIndex(typed))),
        )
        .spacing(6)
        .align_y(Alignment::Center);
    if let Some(browse) = browse::button(PathField::ButtonIcon, icon_path, true) {
        symbol = symbol.push(browse);
    }
    symbol = symbol.push(layout::secondary_button(
        t!("symbol_picker.open"),
        Some(win(WinMsg::ChooseSymbol)),
    ));
    let tooltip = field(t!("button_bar.tooltip_placeholder"), &button.tooltip)
        .on_input(|typed| win(WinMsg::Tooltip(typed)));
    rows = rows
        .push(labeled(t!("button_bar.field.icon"), symbol))
        .push(help(t!("button_bar.help.icon"), tokens))
        .push(labeled(t!("button_bar.field.tooltip"), tooltip));
    scrollable(rows.padding([0, 12]))
        .height(Length::Fill)
        .into()
}

/// The import: the file, Read, and what it would make, bar by bar and button by button.
fn import_page<'a>(import: &'a Import, tokens: &'static Tokens) -> Element<'a, Message> {
    let path = field(t!("button_bar.import_placeholder"), &import.path)
        .on_input(|typed| win(WinMsg::ImportPath(typed)))
        .on_submit(win(WinMsg::ImportRead));
    let mut line = Row::new().push(path).spacing(6).align_y(Alignment::Center);
    if let Some(browse) = browse::button(PathField::BarImport, &import.path, true) {
        line = line.push(browse);
    }
    line = line.push(layout::secondary_button(
        t!("button_bar.import_read"),
        (!import.path.trim().is_empty()).then(|| win(WinMsg::ImportRead)),
    ));
    let preview: Element<'a, Message> = match &import.preview {
        Some(preview) => scrollable(preview_rows(preview, tokens))
            .height(Length::Fill)
            .into(),
        None => fit::hint(t!("button_bar.import_hint"), tokens).into(),
    };
    let buttons = row![
        Space::new().width(Length::Fill),
        layout::primary_icon_button(
            Icon::Import,
            t!("button_bar.import_run"),
            import.preview.as_ref().map(|_| win(WinMsg::ImportRun)),
        ),
        layout::secondary_button(
            t!("button_bar.import_cancel"),
            Some(win(WinMsg::ImportCancel))
        ),
    ]
    .spacing(8);
    column![
        labeled(t!("button_bar.field.import"), line),
        preview,
        buttons
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}

/// One line per button of each bar of `preview`: its tooltip and what it becomes.
fn preview_rows<'a>(preview: &ImportPreview, tokens: &'static Tokens) -> Column<'a, Message> {
    let mut rows = Column::new().spacing(2);
    for bar in &preview.bars {
        let title = t!(
            "button_bar.import_bar",
            name = bar.name.as_str(),
            file = bar.file.as_str()
        );
        rows = rows.push(fit::label(title).font(theme::UI_FONT_STRONG));
        for item in &bar.items {
            let result = match &item.result {
                PreviewResult::Command { action, unknown } if unknown.is_empty() => {
                    action.command.clone()
                }
                PreviewResult::Command { action, unknown } => {
                    t!(
                        "button_bar.import_unknown",
                        command = action.command.as_str(),
                        names = unknown.join(", ")
                    )
                }
                PreviewResult::SubBar { bar } => {
                    let name = preview.bars.get(*bar).map_or("", |sub| sub.name.as_str());
                    t!("button_bar.opens", name = name)
                }
                PreviewResult::Separator => t!("button_bar.separator").to_owned(),
                PreviewResult::Skipped { skip } => {
                    let why = crate::i18n::code_text("tc_import.skip", skip.code());
                    t!("button_bar.import_skipped", why = why)
                }
            };
            let line = row![
                fit::cut(item.tooltip.clone(), theme::UI_FONT, None, 2, tokens),
                fit::cut(
                    result,
                    theme::UI_FONT,
                    Some(tokens.text_secondary),
                    3,
                    tokens
                ),
            ]
            .spacing(8);
            rows = rows.push(line);
        }
        if bar.dropped > 0 {
            rows = rows.push(fit::hint(
                t!("button_bar.import_dropped", count = bar.dropped),
                tokens,
            ));
        }
    }
    rows
}
