//! Drawing the Shortcuts tab: the help line, the search, the table of keys - one stop of the
//! focus ring -, the recording's prompt, the question before a key moves or the panel of "Add key
//! for a theme..." (RC-100-55), "Add key for a workspace..." (RC-100-68) or "Add key for an own
//! command..." and "...for a Start menu entry..." (RC-100-86), and the tab's buttons.

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, container, mouse_area, pick_list, row, scrollable, text_input},
};
use rc_engine::{KeyContext, KeymapRow};

use super::{
    draft::Draft,
    shortcuts::{
        CommandKeyKind, CommandKeyMsg, CommandOption, Recording, Selected, ShortcutsMsg,
        ShortcutsPage, ThemeKeyMsg, ThemeOption, WorkspaceKeyMsg, WorkspaceOption, command_options,
        command_panel, keymap_of, label, lock_reason, page_message, row_of, shown, theme_options,
        workspace_options,
    },
    view::hint,
};
use crate::{
    app::{
        Message,
        palette::{
            themes::{self, ThemeRow},
            user_commands,
            workspaces::{self, WorkspaceRow},
        },
        user_commands::known,
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The columns' shares of the width: action, key, where it applies, the mark.
const PORTIONS: [u16; 4] = [6, 3, 2, 2];

/// The own commands and the Start menu's rows the window knows (RC-100-86).
type Lists = (Vec<rc_engine::UserCommand>, Vec<rc_engine::StartItem>);

pub fn view<'a>(
    page: &'a ShortcutsPage,
    draft: &Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let keymap = keymap_of(draft);
    let rows = shown(&keymap, &page.search);
    let known = themes::known();
    let spaces = workspaces::rows();
    // The own commands and the Start menu, for their keys' rows and panel (RC-100-86).
    let lists = (known::commands(), known::start_menu());
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
            scrollable(Column::with_children(rows.iter().enumerate().map(
                |(index, row)| key_row(page, row, index, (&known, &spaces, &lists), tokens),
            )))
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
    // By the selected name: a new theme key has no row before its first key (RC-100-55).
    let name = page
        .selected
        .as_ref()
        .map(|selected| selected.name.as_str());
    if let (Some(_), Some(name)) = (page.recording, name) {
        let prompt = t!("settings.shortcuts.recording", name = label(name));
        body = body.push(fit::prose(prompt).color(tokens.accent));
    }
    if let (Some(conflict), Some(name)) = (&page.asking, name) {
        body = body.push(question(
            &crate::keymap::label(&conflict.chord),
            &conflict.holder,
            name,
        ));
    }
    if let Some(adding) = &page.adding {
        body = body.push(theme_panel(adding.name.as_deref(), &known, tokens));
    }
    if let Some(adding) = &page.adding_workspace {
        body = body.push(workspace_panel(adding.name.as_deref(), &spaces, tokens));
    }
    if let Some(adding) = &page.adding_command {
        let options: Vec<CommandOption> = command_options(adding.kind, &lists.0, &lists.1);
        body = body.push(command_panel(adding, options, tokens));
    }
    let entries = command_options(CommandKeyKind::Start, &[], &lists.1);
    let any = [
        !known.is_empty(),
        !spaces.is_empty(),
        !lists.0.is_empty(),
        !entries.is_empty(),
    ];
    body.push(buttons(page, selected, any)).into()
}

/// One row: the action, its key - the reason in a tooltip when the window keeps it -, where it
/// applies and whether it was changed; a click selects it.
fn key_row<'a>(
    page: &ShortcutsPage,
    row: &KeymapRow,
    index: usize,
    (known, spaces, lists): (&[ThemeRow], &[WorkspaceRow], &Lists),
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
        crate::keymap::label_text(&row.keys)
    };
    let key_cell = fit::cell(keys, key_share, row.keys.is_empty(), tokens);
    let when = match row.when {
        KeyContext::Files => t!("settings.shortcuts.when.files"),
        KeyContext::Always => t!("settings.shortcuts.when.always"),
    };
    // A theme key whose theme was deleted since (RC-100-55).
    let gone = themes::missing(&row.name, known);
    // A workspace key whose workspace was deleted since (RC-100-68).
    let gone_workspace = workspaces::missing(&row.name, spaces);
    // An own command's or a Start menu entry's key whose command or entry is gone (RC-100-86).
    let gone_command = user_commands::missing(&row.name, &lists.0, &lists.1);
    let mark = match (row.locked, row.changed) {
        (Some(_), _) => t!("settings.shortcuts.locked"),
        (None, _) if gone => t!("settings.shortcuts.theme_missing"),
        (None, _) if gone_workspace => t!("settings.shortcuts.workspace_missing"),
        (None, _) if gone_command => t!("settings.shortcuts.user_command_missing"),
        (None, true) => t!("settings.shortcuts.changed"),
        (None, false) => "",
    };
    let mark_cell = fit::cell(mark, mark_share, row.locked.is_some(), tokens);
    let words = themes::label_in(&row.name, known)
        .or_else(|| workspaces::label_in(&row.name, spaces))
        .or_else(|| user_commands::label_in(&row.name, &lists.0, &lists.1))
        .unwrap_or_else(|| label(&row.name));
    let cells = vec![
        fit::cell(words, action, false, tokens),
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

/// "Add key for a theme...": the theme to choose, then "Record key" or "Cancel".
fn theme_panel<'a>(
    chosen: Option<&str>,
    known: &[ThemeRow],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let options = theme_options(known);
    let selected = options
        .iter()
        .find(|option| Some(option.name.as_str()) == chosen)
        .cloned();
    let pick = |option: ThemeOption| page_message(ShortcutsMsg::Theme(ThemeKeyMsg::Pick(option)));
    let caption = t!("settings.shortcuts.theme");
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(Length::Fill);
    let list = layout::keyed(focus::Stop::pick(list, caption, moves), caption);
    let theme_message = |message| Some(page_message(ShortcutsMsg::Theme(message)));
    column![
        fit::prose(t!("settings.shortcuts.theme.help")).color(tokens.text_secondary),
        row![layout::caption(caption, None), list]
            .spacing(8)
            .align_y(Alignment::Center),
        row![
            layout::secondary_button(
                t!("settings.shortcuts.theme.record"),
                chosen.and_then(|_| theme_message(ThemeKeyMsg::Record)),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.theme.cancel"),
                theme_message(ThemeKeyMsg::Cancel),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
    ]
    .spacing(8)
    .into()
}

/// "Add key for a workspace...": the workspace to choose, then "Record key" or "Cancel"
/// (RC-100-68).
fn workspace_panel<'a>(
    chosen: Option<&str>,
    spaces: &[WorkspaceRow],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let options = workspace_options(spaces);
    let selected = options
        .iter()
        .find(|option| Some(option.name.as_str()) == chosen)
        .cloned();
    let pick = |option: WorkspaceOption| {
        page_message(ShortcutsMsg::Workspace(WorkspaceKeyMsg::Pick(option)))
    };
    let caption = t!("settings.shortcuts.workspace");
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(Length::Fill);
    let list = layout::keyed(focus::Stop::pick(list, caption, moves), caption);
    let message = |message| Some(page_message(ShortcutsMsg::Workspace(message)));
    column![
        fit::prose(t!("settings.shortcuts.workspace.help")).color(tokens.text_secondary),
        row![layout::caption(caption, None), list]
            .spacing(8)
            .align_y(Alignment::Center),
        row![
            layout::secondary_button(
                t!("settings.shortcuts.workspace.record"),
                chosen.and_then(|_| message(WorkspaceKeyMsg::Record)),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.workspace.cancel"),
                message(WorkspaceKeyMsg::Cancel),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
    ]
    .spacing(8)
    .into()
}

/// The selected row's buttons, then "Add key for a theme..." while there are themes, "Add key for
/// a workspace..." while there are workspaces, "Add key for an own command..." and "...for a Start
/// menu entry..." while there are such, and "Reset all"; they wait while a key is recorded, the
/// question or a panel is open. A row the window keeps gets no other key and keeps its own.
fn buttons<'a>(
    page: &ShortcutsPage,
    selected: Option<&KeymapRow>,
    [any_theme, any_workspace, any_command, any_entry]: [bool; 4],
) -> Element<'a, Message> {
    let idle = page.recording.is_none()
        && page.asking.is_none()
        && page.adding.is_none()
        && page.adding_workspace.is_none()
        && page.adding_command.is_none();
    let add_command = |any: bool, kind| {
        (idle && any).then(|| page_message(ShortcutsMsg::Command(CommandKeyMsg::Open(kind))))
    };
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
                t!("settings.shortcuts.add_theme"),
                (idle && any_theme).then(|| page_message(ShortcutsMsg::Theme(ThemeKeyMsg::Open))),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.add_workspace"),
                (idle && any_workspace)
                    .then(|| { page_message(ShortcutsMsg::Workspace(WorkspaceKeyMsg::Open)) }),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.reset_all"),
                idle.then(|| page_message(ShortcutsMsg::ResetAll)),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
        row![
            layout::secondary_button(
                t!("settings.shortcuts.add_user_command"),
                add_command(any_command, CommandKeyKind::Own),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.add_start_item"),
                add_command(any_entry, CommandKeyKind::Start),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
    ]
    .spacing(6)
    .into()
}
