//! Drawing the database dialog: a medium card over the dimmed window that takes every click; the
//! import, whose table needs the room, a large one (`exchange_view.rs` draws the export's and
//! the import's own fields); or a window of its own (RC-080-23, `dialog_windows.rs`).

use iced::{
    Alignment, Element, Length,
    widget::{column, container, mouse_area, row, text_input},
};

use super::{DatabaseDialog, DatabaseMsg, DialogMode, ExchangeMsg, Phase, exchange_view};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
    },
    i18n::t,
    theme::{
        self, Tokens,
        fit::{line, prose},
        focus,
        icon::Icon,
        layout,
    },
};

/// The path field, which takes the keyboard when the dialog's window opens.
pub const PATH_FIELD: &str = "rcommander.database.path";

/// The dialog over the window.
pub fn overlay<'a>(dialog: &'a DatabaseDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let head = Message::titled_head(title(dialog));
    if dialog.mode == DialogMode::Import {
        // The table takes the room the window has.
        return layout::large_dialog(head, body(dialog, tokens), tokens);
    }
    layout::dialog_card(
        head,
        body(dialog, tokens),
        layout::MEDIUM_DIALOG_WIDTH,
        tokens,
    )
}

/// The dialog's title by its mode: in its card's head, or in its window's title bar.
pub fn title(dialog: &DatabaseDialog) -> &'static str {
    match dialog.mode {
        DialogMode::Backup => t!("db.title_backup"),
        DialogMode::Restore => t!("db.title_restore"),
        DialogMode::Export => t!("exchange.title_export"),
        DialogMode::Import => t!("exchange.title_import"),
    }
}

/// The dialog's fields and buttons: on its card, or filling a window of its own (RC-080-23).
pub fn body<'a>(dialog: &'a DatabaseDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let running = dialog.phase == Phase::Running;
    let (label, confirm, browsed) = match dialog.mode {
        DialogMode::Backup => (
            t!("db.path_backup"),
            t!("db.confirm_backup"),
            PathField::Backup,
        ),
        DialogMode::Restore => (
            t!("db.path_restore"),
            t!("db.confirm_restore"),
            PathField::Restore,
        ),
        DialogMode::Export => (
            t!("exchange.path_export"),
            t!("exchange.confirm_export"),
            PathField::ConfigExport,
        ),
        DialogMode::Import => (
            t!("exchange.path_import"),
            t!("exchange.confirm_import"),
            PathField::ConfigImport,
        ),
    };
    let tabs = row![
        tab(t!("db.tab_backup"), DialogMode::Backup, dialog, tokens),
        tab(t!("db.tab_restore"), DialogMode::Restore, dialog, tokens),
        tab(
            t!("exchange.tab_export"),
            DialogMode::Export,
            dialog,
            tokens
        ),
        tab(
            t!("exchange.tab_import"),
            DialogMode::Import,
            dialog,
            tokens
        ),
    ]
    .spacing(6);
    // The modes are one stop of the ring; the arrows choose (RC-080-16).
    let tabs: Element<'_, Message> = if running {
        tabs.into()
    } else {
        focus::pick(
            tabs,
            "db.tabs",
            &[
                DialogMode::Backup,
                DialogMode::Restore,
                DialogMode::Export,
                DialogMode::Import,
            ],
            Some(&dialog.mode),
            |mode| Message::Database(DatabaseMsg::Mode(mode)),
        )
    };
    let mut field = text_input(t!("db.placeholder"), &dialog.path)
        .id(PATH_FIELD)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8]);
    if !running {
        field = field.on_input(|path| Message::Database(DatabaseMsg::Path(path)));
    }
    let read = (dialog.mode == DialogMode::Import).then(|| {
        layout::secondary_button(
            t!("exchange.read"),
            (!running).then_some(Message::Database(DatabaseMsg::Exchange(ExchangeMsg::Read))),
        )
    });
    let mut body = column![
        tabs,
        layout::caption(label, Some(tokens.text_secondary)),
        row![
            layout::keyed(focus::Stop::field(field, label), label),
            browse::button(browsed, &dialog.path, !running),
            read
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    match dialog.mode {
        DialogMode::Backup | DialogMode::Export => {
            body = body.push(layout::check(
                t!("db.overwrite"),
                dialog.overwrite,
                !running,
                |on| Message::Database(DatabaseMsg::Overwrite(on)),
            ));
        }
        DialogMode::Restore => {
            body = body.push(prose(t!("db.restore_warning")).color(tokens.text));
        }
        DialogMode::Import => {}
    }
    body = match dialog.mode {
        DialogMode::Backup | DialogMode::Restore => {
            body.push(prose(t!("db.note")).color(tokens.text_secondary))
        }
        DialogMode::Export => exchange_view::export(body, dialog, tokens),
        DialogMode::Import => exchange_view::import(body, dialog, tokens),
    };
    body = body.push(exchange_view::problem(dialog));
    match &dialog.phase {
        Phase::Editing => {}
        Phase::Running => {
            let running = match dialog.mode {
                DialogMode::Backup => t!("db.running_backup"),
                DialogMode::Restore => t!("db.running_restore"),
                DialogMode::Export => t!("exchange.running_export"),
                DialogMode::Import => t!("exchange.running_import"),
            };
            body = body.push(prose(running).color(tokens.text_secondary));
        }
        Phase::Done(result) => body = body.push(prose(result.as_str()).color(tokens.accent)),
        Phase::Failed(error) => {
            body = body.push(prose(error.as_str()).color(theme::DANGER));
        }
    }
    let finished = matches!(dialog.phase, Phase::Done(_));
    let ready = dialog.mode != DialogMode::Import || exchange_view::can_import(dialog);
    let buttons = layout::button_row(
        Some(layout::primary_button(
            confirm,
            (!running && !finished && ready).then(|| Message::Database(DatabaseMsg::Confirm)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("db.close"),
            Some(Message::Database(DatabaseMsg::Close)),
        )],
        exchange_view::choose_all(dialog),
    );
    if dialog.mode == DialogMode::Import {
        // The table takes the room the window has.
        return body.push(buttons).height(Length::Fill).into();
    }
    body.push(buttons).into()
}

/// One of the modes; the current one is drawn like the panel cursor.
fn tab<'a>(
    label: &'static str,
    mode: DialogMode,
    dialog: &DatabaseDialog,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let current = dialog.mode == mode;
    let tab = container(line(label))
        .padding([4, 12])
        .style(move |_| theme::row(tokens, current, true, false));
    if current || dialog.phase == Phase::Running {
        tab.into()
    } else {
        mouse_area(tab)
            .on_press(Message::Database(DatabaseMsg::Mode(mode)))
            .into()
    }
}
