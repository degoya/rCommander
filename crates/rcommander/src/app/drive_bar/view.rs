//! Drawing the drive bars, the drive lines and the open drive menu.
//!
//! A drive button shows a symbol for the drive's kind and its letter or name; the panel's current
//! drive is filled. Its tooltip, and the drive line under the bar (`line.rs`), tell the volume
//! name and the free space. The symbols are Unicode characters the system's fonts draw (Segoe UI
//! Symbol and Segoe UI Emoji on Windows), so no icon files ship. A folder of the connection tree
//! is one button that opens the menu at its servers; in the menu its servers stand under its
//! name. A network drive is named by the folder it is mapped to; a right click on a drive opens
//! its context menu, which `network_view.rs` draws with the network drive dialogs.

use iced::{
    Alignment, Background, Border, Element, Length, Padding, Shadow, Vector,
    mouse::Interaction,
    widget::{
        Row, Space, column, container, mouse_area, responsive, row, scrollable, stack, text,
        tooltip::{self, Tooltip},
    },
};
use rc_engine::{Drive, DriveGroup, DriveKind, DriveState, PanelId};

use super::{
    DriveBarLayout, DriveMenu, DriveMessage, MENU_SCROLL_ID, line,
    menu::{MenuLine, lines},
    network_view, overflow,
};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{self, Tokens, fit},
};

/// Where the open menu starts below the window's top: under the bar and the drive line.
pub(super) const MENU_TOP: f32 = 70.0;
const MENU_WIDTH: f32 = 360.0;
const MENU_MAX_HEIGHT: f32 = 440.0;
pub(super) const SMALL: f32 = 12.0;
/// The space between two groups of the bar (drives, WSL, servers).
const GROUP_GAP: f32 = 10.0;
/// A connection-tree folder's button: its symbol and the arrow of its menu (the drive line's too).
const FOLDER: &str = "\u{1f4c1}";
/// How far a server in a folder of the menu stands in from the folder's heading.
const FOLDER_INDENT: f32 = 16.0;
pub(super) const FOLDER_ARROW: &str = "\u{25be}";
/// The overflow button of a bar too narrow for its drives.
const MORE: &str = "\u{bb}";

/// What goes above both panels: the one shared bar, or nothing.
pub(in crate::app) fn above_panels<'a>(
    app: &'a App,
    tokens: &'static Tokens,
) -> Vec<Element<'a, Message>> {
    match app.drive_bar.layout {
        DriveBarLayout::Shared => vec![bar(app, None, tokens)],
        DriveBarLayout::PerPanel => Vec::new(),
    }
}

/// What goes above one panel: its own bar when each has one, and its drive line.
pub(in crate::app) fn above_panel<'a>(
    app: &'a App,
    side: PanelId,
    tokens: &'static Tokens,
) -> Vec<Element<'a, Message>> {
    let mut parts = Vec::with_capacity(2);
    if app.drive_bar.layout == DriveBarLayout::PerPanel {
        parts.push(bar(app, Some(side), tokens));
    }
    parts.push(line::drive_line(app, side, tokens));
    parts
}

/// `base` with the open drive menu over it, a click beside the menu closing it; then a drive's
/// context menu or a network drive dialog over that.
pub(in crate::app) fn with_menu<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let base = menu_layer(app, base, tokens);
    network_view::with_network(app, base, tokens)
}

fn menu_layer<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(menu) = &app.drive_bar.menu else {
        return base;
    };
    let card = container(menu_card(app, menu, tokens)).width(Length::FillPortion(1));
    let beside = Space::new().width(Length::FillPortion(1));
    let placed = match menu.panel {
        PanelId::Left => row![card, beside],
        PanelId::Right => row![beside, card],
    };
    let layer = mouse_area(
        container(placed)
            .padding(iced::Padding {
                top: MENU_TOP,
                right: 16.0,
                bottom: 16.0,
                left: 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::Drive(DriveMessage::CloseMenu));
    stack![base, layer].into()
}

/// A bar of drive buttons acting on `panel`, or on the active panel when shared: as many as fit
/// its width, in their order, the rest behind `»` (owner, 2026-10-18: a bar is never cut off
/// unseen). The right-most bar ends with the layout button.
fn bar<'a>(app: &'a App, panel: Option<PanelId>, tokens: &'static Tokens) -> Element<'a, Message> {
    let target = panel.unwrap_or(app.active);
    let buttons = responsive(move |size| {
        let items = bar_items(app, target, tokens);
        let widths: Vec<f32> = items.iter().map(|(_, width)| *width).collect();
        let more = overflow::button_width(&[MORE], SMALL);
        let shown = overflow::visible_count(&widths, size.width, more);
        let hidden = shown < items.len();
        let mut line = Row::with_children(items.into_iter().take(shown).map(|(item, _)| item))
            .spacing(overflow::SPACING)
            .align_y(Alignment::Center);
        if hidden {
            line = line.push(more_button(target, tokens));
        }
        container(line).width(Length::Fill).clip(true).into()
    })
    .height(Length::Shrink);
    let mut line = row![buttons].align_y(Alignment::Center).spacing(6);
    if panel != Some(PanelId::Left) {
        line = line.push(layout_button(app.drive_bar.layout, tokens));
    }
    line.into()
}

/// The bar's buttons with their estimated widths, a gap between two groups.
fn bar_items<'a>(
    app: &'a App,
    target: PanelId,
    tokens: &'static Tokens,
) -> Vec<(Element<'a, Message>, f32)> {
    let current = app.current_drive(target);
    let mut items: Vec<(Element<'a, Message>, f32)> = Vec::new();
    let mut group = None;
    // The top-level folders drawn so far: each is one button, whatever it holds.
    let mut folders: Vec<&str> = Vec::new();
    for drive in &app.drive_bar.drives {
        // A server is a button of its own when marked; its folder's button reaches it anyway.
        let own = drive.group != DriveGroup::Servers || drive.favourite;
        let folder = drive
            .folder
            .first()
            .map(String::as_str)
            .filter(|top| !folders.contains(top));
        if !own && folder.is_none() {
            continue;
        }
        if group.is_some_and(|group| group != drive.group) {
            items.push((Space::new().width(GROUP_GAP).into(), GROUP_GAP));
        }
        group = Some(drive.group);
        if let Some(top) = folder {
            folders.push(top);
            let inside = current
                .is_some_and(|current| current.folder.first().map(String::as_str) == Some(top));
            let width = overflow::button_width(&[FOLDER, top, FOLDER_ARROW], SMALL);
            items.push((folder_button(top, target, inside, tokens), width));
        }
        if own {
            let shown = current.is_some_and(|current| current.id == drive.id);
            let width =
                overflow::button_width(&[kind_symbol(drive.kind), short_label(drive)], SMALL);
            items.push((drive_button(drive, target, shown, tokens), width));
        }
    }
    items
}

/// `»`: the drives that do not fit, in the drive menu.
fn more_button<'a>(target: PanelId, tokens: &'static Tokens) -> Element<'a, Message> {
    let face = container(small(MORE))
        .padding([2, 7])
        .style(move |_| button_style(tokens, false));
    let button = mouse_area(face)
        .on_press(Message::Drive(DriveMessage::OpenMenu(target)))
        .interaction(Interaction::Pointer);
    with_tooltip(button.into(), t!("drives.more").to_owned(), tokens)
}

fn drive_button<'a>(
    drive: &'a Drive,
    target: PanelId,
    current: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let face = container(
        row![small(kind_symbol(drive.kind)), small(short_label(drive))]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .padding([2, 7])
    .style(move |_| button_style(tokens, current));
    let button = mouse_area(face)
        .on_press(Message::Drive(DriveMessage::Pick(target, drive.id.clone())))
        .on_right_press(Message::Drive(DriveMessage::Context(
            target,
            drive.id.clone(),
        )))
        .interaction(Interaction::Pointer);
    with_tooltip(button.into(), tooltip_text(drive), tokens)
}

/// A folder of the connection tree: opens the menu at its servers.
fn folder_button<'a>(
    name: &'a str,
    target: PanelId,
    current: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let face = container(
        row![small(FOLDER), small(name), small(FOLDER_ARROW)]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .padding([2, 7])
    .style(move |_| button_style(tokens, current));
    let button = mouse_area(face)
        .on_press(Message::Drive(DriveMessage::OpenFolder(
            target,
            name.to_owned(),
        )))
        .interaction(Interaction::Pointer);
    with_tooltip(button.into(), t!("drives.folder", name = name), tokens)
}

fn layout_button<'a>(layout: DriveBarLayout, tokens: &'static Tokens) -> Element<'a, Message> {
    let hint = match layout {
        DriveBarLayout::PerPanel => t!("drives.layout_to_shared"),
        DriveBarLayout::Shared => t!("drives.layout_to_per_panel"),
    };
    let face = container(small("\u{21c4}"))
        .padding([2, 7])
        .style(move |_| button_style(tokens, false));
    let button = mouse_area(face)
        .on_press(Message::Drive(DriveMessage::ToggleLayout))
        .interaction(Interaction::Pointer);
    with_tooltip(button.into(), hint.to_owned(), tokens)
}

fn menu_card<'a>(app: &'a App, menu: &DriveMenu, tokens: &'static Tokens) -> Element<'a, Message> {
    let drives = &app.drive_bar.drives;
    let mut items: Vec<Element<'a, Message>> = lines(drives, &menu.actions)
        .into_iter()
        .map(|line| match line {
            MenuLine::Group(group) => group_header(group, tokens),
            MenuLine::Folder(folder) => folder_header(folder, tokens),
            MenuLine::Entry(index) => {
                menu_row(&drives[index], menu.panel, index == menu.cursor, tokens)
            }
            MenuLine::Actions => {
                container(small(t!("drives.group_network")).color(tokens.text_secondary))
                    .padding([4, 8])
                    .into()
            }
            MenuLine::Action(index, action) => {
                network_view::action_row(action, menu.panel, None, index == menu.cursor, tokens)
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
        .width(MENU_WIDTH)
        .max_height(MENU_MAX_HEIGHT)
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
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let line = row![
        fit::line(kind_symbol(drive.kind)).width(22),
        fit::line(display_name(drive)).width(Length::Fill),
        fit::line(line_text(drive)).color(tokens.text_secondary),
    ]
    .spacing(6)
    .padding(Padding::from([0, 8]).left(if drive.folder.is_empty() {
        8.0
    } else {
        8.0 + FOLDER_INDENT
    }))
    .align_y(Alignment::Center);
    let face = container(line)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::row(tokens, cursor, true, false));
    mouse_area(face)
        .on_press(Message::Drive(DriveMessage::Pick(panel, drive.id.clone())))
        .on_right_press(Message::Drive(DriveMessage::Context(
            panel,
            drive.id.clone(),
        )))
        .interaction(Interaction::Pointer)
        .into()
}

pub(super) fn with_tooltip<'a>(
    content: Element<'a, Message>,
    hint: String,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let tip = container(small(hint)).padding([4, 8]);
    Tooltip::new(content, tip, tooltip::Position::Bottom)
        .gap(4)
        .style(move |_| card_style(tokens))
        .into()
}

/// The symbol of a drive's kind.
pub(super) fn kind_symbol(kind: DriveKind) -> &'static str {
    match kind {
        DriveKind::Fixed => "\u{1f5b4}",
        DriveKind::Removable => "\u{23cf}",
        DriveKind::Network => "\u{1f5a7}",
        DriveKind::Optical => "\u{1f4bf}",
        DriveKind::Ram => "\u{26a1}",
        DriveKind::Unknown => "\u{25ab}",
        DriveKind::Wsl => "\u{1f427}",
        DriveKind::Server => "\u{1f5a5}",
    }
}

/// What a button shows: a drive's letter without its colon, a name as it is.
pub(super) fn short_label(drive: &Drive) -> &str {
    match drive.group {
        DriveGroup::Drives if drive.label.len() > 1 => drive.label.trim_end_matches(':'),
        _ => &drive.label,
    }
}

/// A network drive's folder or the volume name with the drive's label, or the label alone.
pub(super) fn display_name(drive: &Drive) -> String {
    match drive.remote.as_ref().or(drive.volume.as_ref()) {
        Some(name) => t!("drives.volume_label", volume = name, label = drive.label),
        None => drive.label.clone(),
    }
}

/// The free space, or why there is none, for the drive line and the menu.
pub(super) fn line_text(drive: &Drive) -> String {
    match (drive.state, drive.space) {
        (DriveState::Ready, Some(space)) => t!(
            "drives.free_of",
            free = human_size(space.free),
            total = human_size(space.total)
        ),
        (DriveState::NotReady, _) => t!("drives.not_ready").to_owned(),
        (DriveState::Waiting, _) => t!("drives.waiting").to_owned(),
        _ => String::new(),
    }
}

/// The tooltip of a drive button.
pub(super) fn tooltip_text(drive: &Drive) -> String {
    let name = display_name(drive);
    match line_text(drive) {
        info if info.is_empty() => name,
        info => t!("drives.tooltip", name = name, info = info),
    }
}

/// A size as the file list writes it (`format::size`): 1024-based, `KiB` and up.
pub(super) fn human_size(bytes: u64) -> String {
    crate::format::size(bytes)
}

pub(super) fn small<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content)
        .size(SMALL)
        .font(theme::UI_FONT)
        .wrapping(text::Wrapping::None)
}

/// A drive button: filled when it is the panel's current drive.
pub(super) fn button_style(tokens: &Tokens, current: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(if current {
            tokens.cursor_fill
        } else {
            tokens.control
        })),
        border: Border::default().rounded(4.0).width(1.0).color(if current {
            tokens.accent
        } else {
            tokens.stroke
        }),
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

/// The drive menu and the tooltips: a raised card.
pub(super) fn card_style(tokens: &Tokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.surface)),
        border: Border::default()
            .rounded(6.0)
            .width(1.0)
            .color(tokens.stroke),
        shadow: Shadow {
            color: iced::Color {
                a: if tokens.dark { 0.45 } else { 0.16 },
                ..iced::Color::BLACK
            },
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        text_color: Some(tokens.text),
        snap: true,
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
