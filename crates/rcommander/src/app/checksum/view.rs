//! Drawing the checksum windows: a card over the dimmed window that takes every click, like the
//! archive dialogs; the verification results a large dialog that fills the window. In windows
//! of their own the results and the forms draw there (RC-080-15, RC-080-23).

use iced::{
    Alignment, Element, Length,
    widget::{column, pick_list, row, scrollable, text_input},
};
use rc_engine::Algorithm;

use super::{
    AlgorithmChoice, ChecksumMsg, ChecksumWindow, CombineDialog, CreateDialog, SizePreset,
    SplitDialog, report::Section,
};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
    },
    format,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{line, prose},
        focus,
        icon::Icon,
        layout,
    },
};

/// The width of a result row's status.
const STATUS_WIDTH: f32 = 110.0;

fn wrap(message: ChecksumMsg) -> Message {
    Message::Checksum(message)
}

/// The first field of a form - the checksum file's name, the target folder -, which takes the
/// keyboard when the form's window opens.
pub const FIRST_FIELD: &str = "rcommander.checksum.first";

/// The open window over the window.
pub fn overlay<'a>(window: &'a ChecksumWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let head = Message::titled_head(title(window));
    let content = body(window, tokens);
    // The results are a list to read through: a large dialog; the forms are medium cards.
    if let ChecksumWindow::Results(_) = window {
        return theme::large_dialog(head, content, tokens);
    }
    layout::dialog_card(head, content, layout::MEDIUM_DIALOG_WIDTH, tokens)
}

/// The window's title: in its card's head, or in its own window's title bar.
pub fn title(window: &ChecksumWindow) -> &'static str {
    match window {
        ChecksumWindow::Create(_) => t!("checksum.create.title"),
        ChecksumWindow::Split(_) => t!("checksum.split.title"),
        ChecksumWindow::Combine(_) => t!("checksum.combine.title"),
        ChecksumWindow::Results(_) => t!("checksum.result.title"),
    }
}

/// The form's fields or the result list, with the buttons: on the card, or filling a window of
/// their own.
pub fn body<'a>(window: &'a ChecksumWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    match window {
        ChecksumWindow::Create(create) => create_view(create, tokens),
        ChecksumWindow::Split(split) => split_view(split, tokens),
        ChecksumWindow::Combine(combine) => combine_view(combine, tokens),
        ChecksumWindow::Results(sections) => results_view(sections, tokens),
    }
}

fn create_view<'a>(create: &'a CreateDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let algorithms: Vec<AlgorithmChoice> =
        Algorithm::ALL.into_iter().map(AlgorithmChoice).collect();
    let chosen = AlgorithmChoice(create.algorithm);
    let choose = |choice| wrap(ChecksumMsg::Algorithm(choice));
    let moves = focus::moves(&algorithms, Some(&chosen), &choose);
    let list = pick_list(algorithms, Some(chosen), choose).width(220);
    let algorithm = t!("checksum.create.algorithm");
    let choice = row![
        layout::caption(algorithm, None),
        layout::keyed(focus::Stop::pick(list, algorithm, moves), algorithm),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let per_file = layout::check(
        t!("checksum.create.per_file"),
        create.per_file,
        true,
        |on| wrap(ChecksumMsg::PerFile(on)),
    );
    let mut name = field(&create.name).id(FIRST_FIELD);
    if !create.per_file {
        name = name.on_input(|name| wrap(ChecksumMsg::Name(name)));
    }
    let count = create.names.len().to_string();
    let ok = create.name_ok();
    let file_name = t!("checksum.create.file_name");
    let mut form = column![
        prose(t!("checksum.create.for", count = count)).color(tokens.text_secondary),
        choice,
        per_file,
        layout::caption(file_name, Some(tokens.text_secondary)),
        layout::keyed(focus::Stop::field(name, file_name), file_name),
    ]
    .spacing(10);
    if !ok {
        form = form.push(prose(t!("checksum.create.name_invalid")).color(theme::DANGER));
    }
    form.push(buttons(t!("checksum.create.ok"), ok)).into()
}

fn split_view<'a>(split: &'a SplitDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let part_size = t!("checksum.split.part_size");
    let size = row![
        layout::keyed(
            focus::Stop::field(
                field(&split.part_size)
                    .on_input(|size| wrap(ChecksumMsg::PartSize(size)))
                    .width(Length::Fill),
                part_size
            ),
            part_size
        ),
        focus::pick(
            pick_list(SizePreset::ALL, None::<SizePreset>, |preset| {
                wrap(ChecksumMsg::Preset(preset))
            })
            .placeholder(t!("checksum.split.presets"))
            .width(220),
            t!("checksum.split.presets"),
            &SizePreset::ALL,
            None,
            |preset| wrap(ChecksumMsg::Preset(preset)),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let (hint, ok) = match split.parts() {
        Ok((part, parts)) => (
            prose(t!(
                "checksum.split.parts",
                parts = parts,
                size = format::size(part),
            ))
            .color(tokens.text_secondary),
            true,
        ),
        Err(code) => (prose(error_text(code)).color(theme::DANGER), false),
    };
    column![
        prose(t!(
            "checksum.split.file",
            name = split.file,
            size = format::size(split.size),
        ))
        .color(tokens.text_secondary),
        layout::caption(t!("checksum.split.target"), Some(tokens.text_secondary)),
        target(&split.target, t!("checksum.split.target")),
        layout::caption(part_size, Some(tokens.text_secondary)),
        size,
        hint,
        prose(t!("checksum.split.crc_note")).color(tokens.text_secondary),
        buttons(t!("checksum.split.ok"), ok),
    ]
    .spacing(10)
    .into()
}

fn combine_view<'a>(combine: &'a CombineDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let name = text_input(t!("checksum.combine.name_placeholder"), &combine.name)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8])
        .on_input(|name| wrap(ChecksumMsg::Name(name)))
        .on_submit(wrap(ChecksumMsg::Confirm));
    column![
        prose(t!("checksum.combine.file", name = combine.file)).color(tokens.text_secondary),
        layout::caption(t!("checksum.combine.target"), Some(tokens.text_secondary)),
        target(&combine.target, t!("checksum.combine.target")),
        layout::caption(t!("checksum.combine.name"), Some(tokens.text_secondary)),
        layout::keyed(
            focus::Stop::field(name, t!("checksum.combine.name")),
            t!("checksum.combine.name")
        ),
        prose(t!("checksum.combine.crc_note")).color(tokens.text_secondary),
        buttons(t!("checksum.combine.ok"), true),
    ]
    .spacing(10)
    .into()
}

/// The result list with its Close: in the large dialog, or filling a window of its own
/// (RC-080-15, `dialog_windows.rs`).
pub fn results_view<'a>(sections: &'a [Section], tokens: &'static Tokens) -> Element<'a, Message> {
    let danger = theme::DANGER;
    let mut list = column![].spacing(4);
    for section in sections {
        list = list.push(prose(section.title.as_str()).font(theme::UI_FONT_STRONG));
        for found in &section.rows {
            let status = line(found.status.as_str())
                .width(STATUS_WIDTH)
                .color(if found.bad {
                    danger
                } else {
                    tokens.text_secondary
                });
            let mut entry = column![prose(found.name.as_str())];
            if !found.detail.is_empty() {
                entry = entry.push(prose(found.detail.as_str()).color(tokens.text_secondary));
            }
            list = list.push(row![status, entry.width(Length::Fill)].spacing(8));
        }
        if section.omitted > 0 {
            let more = t!("checksum.result.omitted", count = section.omitted);
            list = list.push(prose(more).color(tokens.text_secondary));
        }
    }
    let close = layout::button_row(
        None,
        [layout::secondary_icon_button(
            Icon::Close,
            t!("checksum.result.close"),
            Some(wrap(ChecksumMsg::Close)),
        )],
        [],
    );
    column![
        scrollable(list.padding([0, 12])).height(Length::Fill),
        close,
    ]
    .spacing(10)
    .into()
}

/// The target folder of split and combine, with "Browse...", answering to the access key of
/// its caption `label`.
fn target<'a>(value: &'a str, label: &'static str) -> Element<'a, Message> {
    let field = field(value)
        .id(FIRST_FIELD)
        .on_input(|target| wrap(ChecksumMsg::Target(target)));
    row![
        layout::keyed(focus::Stop::field(field, label), label),
        browse::button(PathField::ChecksumTarget, value, true),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// A text field that confirms the dialog on Enter; editable once it has `on_input`.
fn field(value: &str) -> text_input::TextInput<'_, Message> {
    text_input("", value)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8])
        .on_submit(wrap(ChecksumMsg::Confirm))
}

/// OK (`confirm`, the primary, disabled unless `enabled`) and Cancel at the right.
fn buttons<'a>(confirm: &'static str, enabled: bool) -> Element<'a, Message> {
    layout::button_row(
        Some(layout::primary_button(
            confirm,
            enabled.then(|| wrap(ChecksumMsg::Confirm)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("checksum.cancel"),
            Some(wrap(ChecksumMsg::Close)),
        )],
        [],
    )
}
