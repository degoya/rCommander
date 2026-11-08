//! Drawing the archive dialogs: a card over the dimmed window that takes every click - packing
//! and unpacking medium forms, the password a small prompt; each in a window of its own instead
//! (RC-080-23, `dialog_windows.rs`, the password as a question, `dialog_windows/questions.rs`).

use iced::{
    Alignment, Element,
    widget::{column, pick_list, row, text_input},
};
use rc_vault::ExposeSecret as _;

use super::{
    ArchiveDialog, ArchiveMsg, CONFLICT_PRESETS, DeletePrompt, ExtractDialog, FormatChoice,
    PackDialog, PackOption, PackStep, PasswordPrompt, Typed, wrap,
};
use crate::{
    app::Message,
    i18n::t,
    theme::{
        self, Tokens,
        fit::{line, prose},
        focus,
        icon::Icon,
        layout,
    },
};

/// The field of the archive's name or of the target folder, which takes the keyboard when the
/// dialog's window opens.
pub const TARGET_FIELD: &str = "rcommander.archive.target";

/// The password field, which takes the keyboard when the password's window opens (RC-080-23).
pub const PASSWORD_FIELD: &str = "rcommander.archive.password";

/// The compression levels offered, as the pick list holds them.
const LEVELS: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

/// The open dialog over the window.
pub fn overlay<'a>(dialog: &'a ArchiveDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let width = match dialog {
        ArchiveDialog::Password(_) | ArchiveDialog::Delete(_) => layout::SMALL_DIALOG_WIDTH,
        _ => layout::MEDIUM_DIALOG_WIDTH,
    };
    Message::overlay(title(dialog), body(dialog, tokens), width, tokens)
}

/// The dialog's title: in its card's head, or in its window's title bar.
pub fn title(dialog: &ArchiveDialog) -> &'static str {
    match dialog {
        ArchiveDialog::Pack(pack) if pack.adding => t!("archive_ops.add_title"),
        ArchiveDialog::Pack(_) => t!("archive_ops.pack_title"),
        ArchiveDialog::Delete(_) => t!("archive_ops.delete_title"),
        ArchiveDialog::Extract(_) => t!("archive_ops.extract_title"),
        ArchiveDialog::Password(_) => t!("archive_ops.password_title"),
    }
}

/// The dialog's fields and buttons: on its card, or filling a window of its own.
pub fn body<'a>(dialog: &'a ArchiveDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    match dialog {
        ArchiveDialog::Pack(pack) => pack_view(pack, tokens),
        ArchiveDialog::Extract(extract) => extract_view(extract, tokens),
        ArchiveDialog::Password(prompt) => password_view(prompt, tokens),
        ArchiveDialog::Delete(prompt) => delete_view(prompt, tokens),
    }
}

fn pack_view<'a>(pack: &'a PackDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let editing = pack.step == PackStep::Editing;
    let count = pack.sources.len().to_string();
    let mut name = text_input("", &pack.name)
        .id(TARGET_FIELD)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8]);
    // Adding (RC-110-05): the place inside the archive is the other panel's, not typed.
    if editing && !pack.adding {
        name = name.on_input(|name| wrap(ArchiveMsg::Name(name)));
    }
    let formats: Vec<FormatChoice> = rc_engine::PackFormat::ALL
        .into_iter()
        .map(FormatChoice)
        .collect();
    let format = FormatChoice(pack.settings.format);
    let choose_format = |choice| wrap(ArchiveMsg::Format(choice));
    let choose_level = |level| wrap(ArchiveMsg::Level(level));
    let format_moves = focus::moves(&formats, Some(&format), &choose_format);
    let format_list = pick_list(formats, Some(format), choose_format).width(120);
    let format_stop: Element<'a, Message> = if pack.adding {
        // The archive's own format: fixed.
        line(pack.settings.format.extension()).into()
    } else {
        layout::keyed(
            focus::Stop::pick(format_list, t!("archive_ops.format"), format_moves),
            t!("archive_ops.format"),
        )
    };
    let level_list = pick_list(&LEVELS[..], Some(pack.settings.level), choose_level).width(70);
    let (format_label, level_label) = (t!("archive_ops.format"), t!("archive_ops.level"));
    let choices = row![
        layout::caption(format_label, None),
        format_stop,
        layout::caption(level_label, None),
        layout::keyed(
            focus::Stop::pick(
                level_list,
                level_label,
                focus::moves(&LEVELS, Some(&pack.settings.level), &choose_level)
            ),
            level_label
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let settings = &pack.settings;
    let mut options = column![
        toggle(
            t!("archive_ops.recurse"),
            settings.recurse,
            PackOption::Recurse,
            editing
        ),
        toggle(
            t!("archive_ops.keep_paths"),
            settings.keep_paths,
            PackOption::KeepPaths,
            editing
        ),
    ]
    .spacing(6);
    if !pack.adding {
        options = options.push(toggle(
            t!("archive_ops.each_own"),
            settings.each_own_archive,
            PackOption::EachOwn,
            editing,
        ));
    }
    let options = options
        .push(toggle(
            t!("archive_ops.solid"),
            settings.solid,
            PackOption::Solid,
            editing,
        ))
        .push(toggle(
            t!("archive_ops.store_links"),
            settings.store_links,
            PackOption::StoreLinks,
            editing,
        ))
        .push(toggle(
            t!("archive_ops.delete_sources"),
            settings.delete_sources,
            PackOption::DeleteSources,
            editing,
        ));
    let heading = if pack.adding {
        t!("archive_ops.add_to", count = count)
    } else {
        t!("archive_ops.pack_to", count = count)
    };
    let mut body = column![
        line(heading).color(tokens.text_secondary),
        name,
        choices,
        options,
    ]
    .spacing(10);
    let confirm = match pack.step {
        PackStep::Editing if pack.adding => t!("archive_ops.add_ok"),
        PackStep::Editing => t!("archive_ops.pack_ok"),
        PackStep::Checking => {
            body = body.push(prose(t!("archive_ops.checking")).color(tokens.text_secondary));
            t!("archive_ops.pack_ok")
        }
        PackStep::Confirm { exists } => {
            if exists {
                body = body.push(prose(t!("archive_ops.confirm_overwrite")).color(tokens.text));
            }
            if pack.settings.delete_sources {
                body = body.push(prose(t!("archive_ops.confirm_delete")).color(tokens.text));
            }
            t!("archive_ops.confirm_yes")
        }
    };
    body.push(buttons(confirm, pack.step != PackStep::Checking))
        .into()
}

fn extract_view<'a>(extract: &'a ExtractDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let count = extract.archives.len().to_string();
    let field = |value: &'a str, message: fn(String) -> ArchiveMsg| {
        text_input("", value)
            .font(theme::UI_FONT)
            .size(theme::TEXT_SIZE)
            .padding([6, 8])
            .on_input(move |text| wrap(message(text)))
    };
    let on_conflict = t!("archive_ops.on_conflict");
    let conflict = row![
        layout::caption(on_conflict, None),
        layout::keyed(
            focus::Stop::pick(
                pick_list(CONFLICT_PRESETS, Some(extract.conflict), |preset| {
                    wrap(ArchiveMsg::Conflict(preset))
                })
                .width(220),
                on_conflict,
                focus::moves(&CONFLICT_PRESETS, Some(&extract.conflict), &|preset| wrap(
                    ArchiveMsg::Conflict(preset)
                ))
            ),
            on_conflict
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mask = t!("archive_ops.mask");
    column![
        line(t!("archive_ops.extract_to", count = count)).color(tokens.text_secondary),
        field(&extract.target, ArchiveMsg::Target).id(TARGET_FIELD),
        layout::caption(mask, Some(tokens.text_secondary)),
        layout::keyed(
            focus::Stop::field(field(&extract.mask, ArchiveMsg::Mask), mask),
            mask
        ),
        check(
            t!("archive_ops.own_folder"),
            extract.own_folder,
            ArchiveMsg::OwnFolder
        ),
        check(
            t!("archive_ops.keep_paths"),
            extract.keep_paths,
            ArchiveMsg::KeepPaths
        ),
        conflict,
        buttons(t!("archive_ops.extract_ok"), true),
    ]
    .spacing(10)
    .into()
}

/// F8 inside an archive (RC-110-05): how many entries go from which archive, for good.
fn delete_view<'a>(prompt: &'a DeletePrompt, tokens: &'static Tokens) -> Element<'a, Message> {
    let count = prompt.entries.len().to_string();
    column![
        prose(t!(
            "archive_ops.delete_question",
            count = count,
            archive = prompt.archive
        ))
        .color(tokens.text),
        prose(t!("archive_ops.delete_warning")).color(theme::DANGER),
        buttons(t!("archive_ops.delete_ok"), true),
    ]
    .spacing(10)
    .into()
}

fn password_view<'a>(prompt: &'a PasswordPrompt, tokens: &'static Tokens) -> Element<'a, Message> {
    let field = text_input(
        t!("archive_ops.password_placeholder"),
        prompt.typed.expose_secret(),
    )
    .id(PASSWORD_FIELD)
    .secure(true)
    .font(theme::UI_FONT)
    .size(theme::TEXT_SIZE)
    .padding([6, 8])
    .on_input(|typed| wrap(ArchiveMsg::Password(Typed::from(typed))))
    .on_submit(wrap(ArchiveMsg::Confirm));
    let mut body = column![
        prose(t!("archive_ops.password_for", archive = prompt.archive)).color(tokens.text),
    ]
    .spacing(10);
    if prompt.wrong {
        body = body.push(prose(t!("archive_ops.password_wrong")).color(theme::DANGER));
    }
    body.push(field)
        .push(prose(t!("archive_ops.password_note")).color(tokens.text_secondary))
        .push(buttons(t!("archive_ops.password_ok"), true))
        .into()
}

/// OK (`confirm`, the primary, disabled unless `enabled`) and Cancel at the right.
fn buttons<'a>(confirm: &'static str, enabled: bool) -> Element<'a, Message> {
    layout::button_row(
        Some(layout::primary_button(
            confirm,
            enabled.then(|| wrap(ArchiveMsg::Confirm)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("archive_ops.cancel"),
            Some(wrap(ArchiveMsg::Close)),
        )],
        [],
    )
}

fn toggle<'a>(
    label: &'static str,
    on: bool,
    option: PackOption,
    enabled: bool,
) -> Element<'a, Message> {
    layout::check(label, on, enabled, move |on| {
        wrap(ArchiveMsg::Option(option, on))
    })
}

fn check<'a>(
    label: &'static str,
    on: bool,
    message: fn(bool) -> ArchiveMsg,
) -> Element<'a, Message> {
    layout::check(label, on, true, move |on| wrap(message(on)))
}
