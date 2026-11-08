//! Drawing the drive bars, the drive lines and the open drive menu.
//!
//! A drive button shows a symbol for the drive's kind and its letter or name; the panel's current
//! drive is filled. Its tooltip, and the drive line under the bar (`line.rs`), tell the volume
//! name and the free space. The symbols are Unicode characters the system's fonts draw (Segoe UI
//! Symbol and Segoe UI Emoji on Windows), so no icon files ship. A folder of the connection tree
//! is one button that opens the menu at its servers; in the menu its servers stand under its
//! name. A network drive is named by the folder it is mapped to; a right click on a drive opens
//! its context menu, which `network_view.rs` draws with the network drive dialogs. The open menu
//! is `menu_view.rs`'s; each button a menu opens from carries the id it is measured by
//! (`place.rs`).

use iced::{
    Alignment, Background, Border, Element, Length, Shadow, Vector,
    mouse::Interaction,
    widget::{
        Row, Space, container, mouse_area, responsive, row, text,
        tooltip::{self, Tooltip},
    },
};
use rc_engine::{Drive, DriveGroup, DriveKind, DriveState, PanelId};

use super::{
    DriveBarLayout, DriveMessage, line, menu_view, network_view,
    overflow::{self, BarItem},
    place::{self, Button},
};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{self, Tokens},
};

pub(super) const SMALL: f32 = 12.0;
/// The space between two groups of the bar (drives, WSL, servers).
const GROUP_GAP: f32 = 10.0;
/// A connection-tree folder's button: its symbol and the arrow of its menu (the drive line's too).
const FOLDER: &str = "\u{1f4c1}";
pub(super) const FOLDER_ARROW: &str = "\u{25be}";
/// The overflow button of a bar too narrow for its drives.
const MORE: &str = "\u{bb}";

/// What goes above both panels: the one shared bar, or nothing.
pub(in crate::app) fn above_panels<'a>(
    app: &'a App,
    tokens: &'static Tokens,
) -> Vec<Element<'a, Message>> {
    // `appearance.drive_bar` hides the drive buttons, never the drive lines (RC-100-87).
    match app.drive_bar.layout {
        DriveBarLayout::Shared if app.button_bar.settings.drive_bar => vec![bar(app, None, tokens)],
        DriveBarLayout::Shared | DriveBarLayout::PerPanel => Vec::new(),
    }
}

/// What goes above one panel: its own bar when each has one, and its drive line.
pub(in crate::app) fn above_panel<'a>(
    app: &'a App,
    side: PanelId,
    tokens: &'static Tokens,
) -> Vec<Element<'a, Message>> {
    let mut parts = Vec::with_capacity(2);
    if app.drive_bar.layout == DriveBarLayout::PerPanel && app.button_bar.settings.drive_bar {
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
    let base = menu_view::menu_layer(app, base, tokens);
    network_view::with_network(app, base, tokens)
}

/// A bar of drive buttons acting on `panel`, or on the active panel when shared: as many as fit
/// its width, in their order, the rest behind `»` (owner, 2026-10-18: a bar is never cut off
/// unseen). The right-most bar ends with the layout button.
fn bar<'a>(app: &'a App, panel: Option<PanelId>, tokens: &'static Tokens) -> Element<'a, Message> {
    let target = panel.unwrap_or(app.active);
    let drives = &app.drive_bar.drives;
    let buttons = responsive(move |size| {
        let items = overflow::bar_items(drives);
        let shown = shown(drives, &items, size.width);
        let current = app.current_drive(target);
        let buttons = items[..shown]
            .iter()
            .map(|item| bar_button(drives, *item, current, target, tokens));
        let mut line = Row::with_children(buttons)
            .spacing(overflow::SPACING)
            .align_y(Alignment::Center);
        if shown < items.len() {
            line = line.push(more_button(target, shown, tokens));
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

/// How many of the bar's `items` fit `width`, the rest behind `»`.
fn shown(drives: &[Drive], items: &[BarItem<'_>], width: f32) -> usize {
    let widths: Vec<f32> = items.iter().map(|item| item_width(drives, *item)).collect();
    let more = overflow::button_width(&[MORE], SMALL);
    overflow::visible_count(&widths, width, more)
}

/// The estimated width of one of the bar's items.
fn item_width(drives: &[Drive], item: BarItem<'_>) -> f32 {
    match item {
        BarItem::Gap => GROUP_GAP,
        BarItem::Folder(path) => {
            overflow::button_width(&[FOLDER, folder_name(path), FOLDER_ARROW], SMALL)
        }
        BarItem::Drive(index) => {
            let drive = &drives[index];
            overflow::button_width(&[kind_symbol(drive.kind), short_label(drive)], SMALL)
        }
    }
}

/// One of the bar's items for `target`, whose drive is `current`.
fn bar_button<'a>(
    drives: &'a [Drive],
    item: BarItem<'a>,
    current: Option<&Drive>,
    target: PanelId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    match item {
        BarItem::Gap => Space::new().width(GROUP_GAP).into(),
        BarItem::Folder(path) => {
            let inside = current.is_some_and(|current| current.folder.starts_with(path));
            folder_button(path, target, inside, tokens)
        }
        BarItem::Drive(index) => {
            let drive = &drives[index];
            let shown = current.is_some_and(|current| current.id == drive.id);
            drive_button(drive, target, shown, tokens)
        }
    }
}

/// `»`: the items that do not fit, all `shown` before them, in a menu of their own.
fn more_button<'a>(target: PanelId, shown: usize, tokens: &'static Tokens) -> Element<'a, Message> {
    let face = container(small(MORE))
        .id(place::id(target, Button::More))
        .padding([2, 7])
        .style(move |_| button_style(tokens, false));
    let button = mouse_area(face)
        .on_press(Message::Drive(DriveMessage::OpenOverflow(target, shown)))
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
    .id(place::id(target, Button::Drive(&drive.id)))
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

/// The name a folder's button shows: the folder's own, its last on the path.
fn folder_name(path: &[String]) -> &str {
    path.last().map_or("", String::as_str)
}

/// A folder of the connection tree, by its path: opens its servers in a menu under it. The
/// tooltip names the whole path.
fn folder_button<'a>(
    path: &'a [String],
    target: PanelId,
    current: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let face = container(
        row![small(FOLDER), small(folder_name(path)), small(FOLDER_ARROW)]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .id(place::id(target, Button::Folder(path)))
    .padding([2, 7])
    .style(move |_| button_style(tokens, current));
    let button = mouse_area(face)
        .on_press(Message::Drive(DriveMessage::OpenFolder(
            target,
            path.to_vec(),
        )))
        .interaction(Interaction::Pointer);
    let name = path.join(" / ");
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
        DriveKind::Device => "\u{1f4f1}",
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
pub(in crate::app) fn button_style(tokens: &Tokens, current: bool) -> container::Style {
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
pub(in crate::app) fn card_style(tokens: &Tokens) -> container::Style {
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
