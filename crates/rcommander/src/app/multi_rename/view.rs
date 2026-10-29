//! Drawing the multi-rename dialog over the dimmed window: Total Commander's layout - masks and
//! their placeholder buttons, search and replace, case and counter, the presets and the log on
//! top, the table `old name -> new name` with its conflicts below, the buttons at the bottom.

use iced::{
    Alignment, Color, Element, Length,
    widget::{Column, Row, Space, checkbox, column, container, mouse_area, pick_list, row, sensor},
};
use rc_engine::{Conflict, MaskField, SUBFOLDER_SEPARATOR};

use super::{
    CaseChoice, Field, Flag, MrMsg, MultiRenameDialog, Placeholder, PresetChoice, Preview, Status,
    controls::{field, field_sized, flag, indented, labelled, small_button, wrap},
};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
    },
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
        icon::Icon,
        layout,
    },
};

const NUMBER_WIDTH: f32 = 70.0;
const CONFLICT_WIDTH: f32 = 300.0;

/// The dialog over the window.
pub fn overlay<'a>(dialog: &'a MultiRenameDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    // A large dialog: the table takes the height the controls leave.
    let head = Message::titled_head(t!("multi_rename.title"));
    theme::large_dialog(head, body(dialog, tokens), tokens)
}

/// The controls, the table and the buttons: the card's content over the window, the whole of
/// its own window (RC-080-15).
pub fn body<'a>(dialog: &'a MultiRenameDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let danger = theme::DANGER;
    column![
        masks(dialog, danger),
        placeholders(),
        search(dialog),
        case_and_counter(dialog),
        presets(dialog),
        log(dialog),
        status(dialog, tokens, danger),
        table(dialog, tokens, danger),
        buttons(dialog),
    ]
    .spacing(8)
    .into()
}

/// The two masks, each with the position of its error underneath.
fn masks(dialog: &MultiRenameDialog, danger: Color) -> Element<'_, Message> {
    let form = &dialog.form;
    let error = |mask: MaskField| match &dialog.preview {
        Preview::Failed(error) => error.mask.and_then(|(field, position)| {
            (field == mask).then(|| {
                t!(
                    "multi_rename.mask_error",
                    position = position,
                    error = error.text
                )
            })
        }),
        _ => None,
    };
    let mut name = column![field(&form.name_mask, Field::NameMask, "[N]")].spacing(2);
    if let Some(error) = error(MaskField::Name) {
        name = name.push(fit::prose(error).color(danger));
    }
    let mut extension =
        column![field(&form.extension_mask, Field::ExtensionMask, "[E]")].spacing(2);
    if let Some(error) = error(MaskField::Extension) {
        extension = extension.push(fit::prose(error).color(danger));
    }
    row![
        labelled(t!("multi_rename.name_mask"), name),
        line(t!("multi_rename.extension_mask")),
        container(extension).width(220),
    ]
    .spacing(8)
    .align_y(Alignment::Start)
    .into()
}

/// The buttons `[N]`, `[N#-#]`, `[C]`, `[YMD]`, `[hms]` and the menu with all the others; each
/// goes into the mask typed in last.
fn placeholders() -> Element<'static, Message> {
    let mut buttons = Row::new().spacing(6).align_y(Alignment::Center);
    for placeholder in Placeholder::BUTTONS {
        buttons = buttons.push(small_button(
            placeholder.label(),
            Some(MrMsg::Insert(placeholder)),
        ));
    }
    let insert = |placeholder| wrap(MrMsg::Insert(placeholder));
    let more = pick_list(Placeholder::MORE, None::<Placeholder>, insert)
        .placeholder(t!("multi_rename.more_placeholders"))
        .text_size(theme::TEXT_SIZE)
        .width(260);
    let more = focus::pick(
        more,
        t!("multi_rename.more_placeholders"),
        &Placeholder::MORE,
        None,
        insert,
    );
    indented(buttons.push(more))
}

fn search(dialog: &MultiRenameDialog) -> Element<'_, Message> {
    let form = &dialog.form;
    column![
        labelled(
            t!("multi_rename.find"),
            row![
                field(&form.find, Field::Find, t!("multi_rename.find_hint")),
                line(t!("multi_rename.replace")),
                field(&form.replace, Field::Replace, ""),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ),
        indented(
            row![
                flag(
                    t!("multi_rename.case_sensitive"),
                    form.case_sensitive,
                    Flag::CaseSensitive
                ),
                flag(
                    t!("multi_rename.first_only"),
                    form.first_only,
                    Flag::FirstOnly
                ),
                flag(
                    t!("multi_rename.with_extension"),
                    form.with_extension,
                    Flag::WithExtension
                ),
                flag(t!("multi_rename.regex"), form.regex, Flag::RegEx),
                flag(
                    t!("multi_rename.substitute"),
                    form.substitute,
                    Flag::Substitute
                ),
            ]
            .spacing(16)
        ),
    ]
    .spacing(6)
    .into()
}

fn case_and_counter(dialog: &MultiRenameDialog) -> Element<'_, Message> {
    let form = &dialog.form;
    let invalid = match dialog.preview {
        Preview::Invalid(field) => Some(field),
        _ => None,
    };
    labelled(
        t!("multi_rename.case"),
        row![
            focus::pick(
                pick_list(CaseChoice::ALL, Some(form.case), |case| wrap(MrMsg::Case(
                    case
                )))
                .text_size(theme::TEXT_SIZE)
                .width(260),
                t!("multi_rename.case"),
                &CaseChoice::ALL,
                Some(&form.case),
                |case| wrap(MrMsg::Case(case)),
            ),
            Space::new().width(16),
            line(t!("multi_rename.counter")),
            number(
                t!("multi_rename.counter_start"),
                &form.counter_start,
                Field::CounterStart,
                invalid
            ),
            number(
                t!("multi_rename.counter_step"),
                &form.counter_step,
                Field::CounterStep,
                invalid
            ),
            number(
                t!("multi_rename.counter_width"),
                &form.counter_width,
                Field::CounterWidth,
                invalid
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
}

/// A counter field, with a word when it is no number.
fn number<'a>(
    label: &'static str,
    value: &'a str,
    field: Field,
    invalid: Option<Field>,
) -> Element<'a, Message> {
    let mut parts = row![line(label), field_sized(value, field, NUMBER_WIDTH)]
        .spacing(6)
        .align_y(Alignment::Center);
    if invalid == Some(field) {
        parts = parts.push(line(t!("multi_rename.not_a_number")));
    }
    parts.into()
}

/// The presets: `<Default>`, the rule last used, the saved ones; a name to save under.
fn presets(dialog: &MultiRenameDialog) -> Element<'_, Message> {
    let mut choices = vec![PresetChoice::Default];
    if dialog.last.is_some() {
        choices.push(PresetChoice::Last);
    }
    choices.extend(
        dialog
            .presets
            .iter()
            .map(|preset| PresetChoice::Named(preset.name.clone())),
    );
    let named = matches!(dialog.preset, PresetChoice::Named(_));
    let choose = |choice| wrap(MrMsg::Preset(choice));
    let moves = focus::moves(&choices, Some(&dialog.preset), &choose);
    labelled(
        t!("multi_rename.presets"),
        row![
            Element::from(focus::Stop::pick(
                pick_list(choices, Some(dialog.preset.clone()), choose)
                    .text_size(theme::TEXT_SIZE)
                    .width(220),
                t!("multi_rename.presets"),
                moves,
            )),
            field(
                &dialog.preset_name,
                Field::PresetName,
                t!("multi_rename.preset_name")
            ),
            layout::secondary_icon_button(
                Icon::Save,
                t!("multi_rename.preset_save"),
                Some(wrap(MrMsg::SavePreset))
            ),
            layout::secondary_icon_button(
                Icon::Rename,
                t!("multi_rename.preset_rename"),
                named.then(|| wrap(MrMsg::RenamePreset))
            ),
            layout::secondary_icon_button(
                Icon::Delete,
                t!("multi_rename.preset_delete"),
                named.then(|| wrap(MrMsg::DeletePreset))
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
}

fn log(dialog: &MultiRenameDialog) -> Element<'_, Message> {
    let toggle = checkbox(dialog.log)
        .label(t!("multi_rename.log"))
        .on_toggle(|on| wrap(MrMsg::Log(on)))
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT);
    let toggle = focus::toggle(
        toggle,
        t!("multi_rename.log"),
        Some(wrap(MrMsg::Log(!dialog.log))),
    );
    indented(
        row![
            toggle,
            field(
                &dialog.log_path,
                Field::LogPath,
                t!("multi_rename.log_path")
            ),
            browse::button(PathField::RenameLog, &dialog.log_path, true),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
}

/// Where the preview and the last run stand.
fn status<'a>(
    dialog: &'a MultiRenameDialog,
    tokens: &'static Tokens,
    danger: Color,
) -> Element<'a, Message> {
    let (message, color) = match (&dialog.status, &dialog.preview) {
        (Status::Busy, _) => (t!("multi_rename.busy").to_owned(), tokens.text_secondary),
        (Status::Error(error), _) => (error.clone(), danger),
        (Status::Info(info), _) => (info.clone(), tokens.accent),
        (Status::None, Preview::Pending) => (
            t!("multi_rename.previewing").to_owned(),
            tokens.text_secondary,
        ),
        (Status::None, Preview::Failed(error)) if error.mask.is_none() => {
            (error.text.clone(), danger)
        }
        (Status::None, Preview::Ready { changes, conflicts }) if *conflicts > 0 => (
            t!(
                "multi_rename.conflicts",
                count = conflicts,
                changes = changes
            ),
            danger,
        ),
        (Status::None, Preview::Ready { changes, .. }) => (
            t!(
                "multi_rename.changes",
                count = changes,
                total = dialog.list.len()
            ),
            tokens.text_secondary,
        ),
        (Status::None, _) => (String::new(), tokens.text_secondary),
    };
    let log = if dialog.log_ready() {
        String::new()
    } else {
        t!("multi_rename.log_absolute").to_owned()
    };
    // Both wrap rather than leave the card.
    row![
        fit::prose(message).color(color),
        fit::prose(log).color(danger).width(Length::Shrink),
    ]
    .spacing(16)
    .into()
}

/// The rows on screen: old name, new name, conflict; a click puts the cursor on one, the wheel
/// scrolls, a click on the old name's header sorts by it.
fn table<'a>(
    dialog: &'a MultiRenameDialog,
    tokens: &'static Tokens,
    danger: Color,
) -> Element<'a, Message> {
    let list = &dialog.list;
    // focus-ring: pointer only (the column header; its access key sorts by the old name too)
    let sort = mouse_area(
        line(t!("multi_rename.column_old"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
    )
    .on_press(wrap(MrMsg::SortByName));
    let header = row![
        sort,
        line(t!("multi_rename.column_new"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
        line(t!("multi_rename.column_conflict"))
            .color(tokens.text_secondary)
            .width(CONFLICT_WIDTH),
    ]
    .spacing(8)
    .padding([0, 6]);
    let rows = list.visible().filter_map(|index| {
        let (old, item) = list.row(index)?;
        let new = item.map_or_else(String::new, |item| shown(&item.new));
        let conflict = item.and_then(|item| item.conflict);
        let reason = conflict.map_or_else(String::new, |conflict| conflict_text(conflict, dialog));
        let color = if conflict.is_some() {
            danger
        } else {
            tokens.text
        };
        // Names are shortened in the middle, the reason cut at its end; each whole in a tooltip when
        // it does not fit.
        let reason_cell = container(line(reason.clone()).color(danger))
            .width(CONFLICT_WIDTH)
            .clip(true);
        let reason_cell = if fit::overflows(&reason, CONFLICT_WIDTH) {
            fit::tip(reason_cell, reason, tokens)
        } else {
            reason_cell.into()
        };
        let cells = row![
            fit::path_cell(old, 1, false, tokens),
            container(fit::path_line(new, color, tokens)).width(Length::Fill),
            reason_cell,
        ]
        .spacing(8)
        .padding([0, 6])
        .align_y(Alignment::Center);
        let cursor = index == list.cursor();
        let framed = container(cells)
            .height(theme::ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| {
                theme::striped_row(tokens, cursor, true, false, theme::alternate(index))
            });
        Some(
            // focus-ring: pointer only (a row of the rows choice stop)
            mouse_area(framed)
                .on_press(wrap(MrMsg::Click(index)))
                .into(),
        )
    });
    let body = container(Column::with_children(rows))
        .height(Length::Fill)
        .width(Length::Fill)
        .clip(true);
    let body = mouse_area(body)
        .on_scroll(|delta| wrap(MrMsg::Scroll(theme::wheel::rows(delta, theme::ROW_HEIGHT))));
    // The table reports its height, so as many rows are drawn as fit.
    let body = sensor(body)
        .on_show(|size| wrap(MrMsg::Viewport(theme::rows_for(size.height))))
        .on_resize(|size| wrap(MrMsg::Viewport(theme::rows_for(size.height))));
    // The rows are one stop of the ring; the dialog moves their cursor with the arrows.
    let body = focus::choice(body, "multi_rename.rows", None, None);
    column![header, body].spacing(4).height(Length::Fill).into()
}

/// A new name as the platform spells a path: `sub\name` on Windows.
fn shown(name: &str) -> String {
    name.replace(SUBFOLDER_SEPARATOR, std::path::MAIN_SEPARATOR_STR)
}

/// Why a row cannot be renamed; a duplicate names the other entry.
fn conflict_text(conflict: Conflict, dialog: &MultiRenameDialog) -> String {
    let reason = error_text(conflict.code());
    match conflict {
        Conflict::Duplicate { other } => match dialog.list.row(other) {
            Some((name, _)) => t!("multi_rename.duplicate_of", reason = reason, name = name),
            None => reason,
        },
        _ => reason,
    }
}

/// Move up and down at the left; Start (the primary), Undo and Close at the right.
fn buttons(dialog: &MultiRenameDialog) -> Element<'_, Message> {
    let idle = !dialog.busy();
    layout::button_row(
        Some(layout::primary_icon_button(
            Icon::Rename,
            t!("multi_rename.start"),
            dialog.can_start().then(|| wrap(MrMsg::Start)),
        )),
        [
            small_button(t!("multi_rename.undo"), idle.then_some(MrMsg::Undo)),
            layout::secondary_icon_button(
                Icon::Close,
                t!("multi_rename.close"),
                Some(wrap(MrMsg::Close)),
            ),
        ],
        [
            small_button(t!("multi_rename.move_up"), Some(MrMsg::MoveUp)),
            small_button(t!("multi_rename.move_down"), Some(MrMsg::MoveDown)),
        ],
    )
}
