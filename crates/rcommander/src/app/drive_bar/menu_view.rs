//! Drawing the open drive menu: a card under the button it was opened from, or where it always
//! stood when the keyboard opened it (`place.rs`), with the lines its scope lists (`menu.rs`).

use iced::{
    Alignment, Element, Length, Padding, Size,
    mouse::Interaction,
    widget::{column, container, mouse_area, row, scrollable, stack},
};
use rc_engine::{Drive, DriveGroup, PanelId};

use super::{
    DriveMenu, DriveMessage, MENU_SCROLL_ID,
    menu::{MenuLine, lines},
    network_view,
    place::{self, Anchor},
    view::{card_style, display_name, kind_symbol, line_text, small},
};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{self, Tokens, fit},
};

const MENU_WIDTH: f32 = 360.0;
const MENU_MAX_HEIGHT: f32 = 440.0;
/// How far a server in a folder of the menu stands in from the folder's heading.
const FOLDER_INDENT: f32 = 16.0;

/// `base` with the open drive menu over it, a click beside the menu closing it.
pub(super) fn menu_layer<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(menu) = &app.drive_bar.menu else {
        return base;
    };
    let layer = match app.drive_bar.anchor {
        // The button is measured in a moment; the card shows there, not here first.
        Anchor::Measuring => return base,
        Anchor::Under(button) => {
            let count = lines(&app.drive_bar.drives, menu).len();
            let size = Size::new(MENU_WIDTH, place::card_height(count, MENU_MAX_HEIGHT));
            place::layer(button, size, move |at| {
                menu_card(app, menu, at.size(), tokens)
            })
        }
        Anchor::Fixed => {
            let size = Size::new(MENU_WIDTH, MENU_MAX_HEIGHT);
            place::fixed(menu.panel, menu_card(app, menu, size, tokens))
        }
    };
    stack![base, layer].into()
}

fn menu_card<'a>(
    app: &'a App,
    menu: &DriveMenu,
    size: Size,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let drives = &app.drive_bar.drives;
    let mut items: Vec<Element<'a, Message>> = lines(drives, menu)
        .into_iter()
        .map(|line| match line {
            MenuLine::Group(group) => group_header(group, tokens),
            MenuLine::Folder(folder) => folder_header(folder, tokens),
            MenuLine::Entry {
                stop,
                drive,
                indent,
            } => menu_row(
                &drives[drive],
                menu.panel,
                stop == menu.cursor,
                indent,
                tokens,
            ),
            MenuLine::Actions => {
                container(small(t!("drives.group_network")).color(tokens.text_secondary))
                    .padding([4, 8])
                    .into()
            }
            MenuLine::Action(stop, action) => {
                network_view::action_row(action, menu.panel, None, stop == menu.cursor, tokens)
            }
        })
        .collect();
    if items.is_empty() {
        items.push(
            container(small(t!("drives.menu_empty")).color(tokens.text_secondary))
                .padding([6, 10])
                .into(),
        );
    }
    container(scrollable(column(items).padding(4)).id(MENU_SCROLL_ID))
        .width(size.width)
        .max_height(size.height)
        .style(move |_| card_style(tokens))
        .into()
}

fn group_header<'a>(group: DriveGroup, tokens: &'static Tokens) -> Element<'a, Message> {
    let title = match group {
        DriveGroup::Drives => t!("drives.group_drives"),
        DriveGroup::Wsl => t!("drives.group_wsl"),
        DriveGroup::Servers => t!("drives.group_servers"),
    };
    container(small(title).color(tokens.text_secondary))
        .padding([4, 8])
        .into()
}

/// A folder's servers in the menu stand indented under its path (`Work / Staging`), which lines
/// up with the group headings.
fn folder_header<'a>(folder: &[String], tokens: &'static Tokens) -> Element<'a, Message> {
    let title = format!("\u{1f4c1} {}", folder.join(" / "));
    container(small(title).color(tokens.text_secondary))
        .padding([4, 8])
        .into()
}

fn menu_row<'a>(
    drive: &'a Drive,
    panel: PanelId,
    cursor: bool,
    indent: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let line = row![
        fit::line(kind_symbol(drive.kind)).width(22),
        fit::line(display_name(drive)).width(Length::Fill),
        fit::line(line_text(drive)).color(tokens.text_secondary),
    ]
    .spacing(6)
    .padding(Padding::from([0, 8]).left(if indent { 8.0 + FOLDER_INDENT } else { 8.0 }))
    .align_y(Alignment::Center);
    let face = container(line)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::row(tokens, cursor, true, false));
    // focus-ring: pointer only (a row of the menu, which the keyboard runs as one list)
    mouse_area(face)
        .on_press(Message::Drive(DriveMessage::Pick(panel, drive.id.clone())))
        .on_right_press(Message::Drive(DriveMessage::Context(
            panel,
            drive.id.clone(),
        )))
        .interaction(Interaction::Pointer)
        .into()
}
