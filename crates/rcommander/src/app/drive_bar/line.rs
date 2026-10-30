//! A panel's drive line, as Total Commander has it (RC-010-22): set off from the window (its own
//! tone and a hairline under it, the active panel's in the accent), the drive as a button with
//! its symbol, its letter or name and an arrow, which opens the drive menu; the volume name in
//! brackets (a server's or a WSL distribution's name), the free space, and at the right end `\`
//! and `..`. These two run what the keymap's rows `root_folder` and `parent_folder` run
//! (`panel.navigate` to `/` and `..`), their tooltips name those rows' keys, and they are disabled
//! where there is no folder above (a root) and while the panel shows its terminal. The settings
//! `panels.drive_line_volume` and `panels.drive_line_buttons` hide the name and the buttons.
//!
//! The name and the free space share what the buttons leave, each shortened in the middle
//! (`docs/design.md`); only then does the whole line show in a tooltip.

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{Row, Space, column, container, mouse_area, responsive, row},
};
use rc_db::Db;
use rc_engine::{
    Command, Drive, DriveGroup, Keymap, PanelId, PanelView, VfsPath,
    settings::{DRIVE_LINE_BUTTONS_KEY, DRIVE_LINE_VOLUME_KEY, stored_bool},
};

use super::{
    DriveMessage,
    overflow::SPACING,
    view::{
        FOLDER_ARROW, SMALL, button_style, kind_symbol, line_text, short_label, small, with_tooltip,
    },
};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{
        self, Tokens,
        ellipsis::{estimate_width, middle_ellipsis, middle_ellipsis_by},
    },
};

/// The space between the line's parts.
const GAP: f32 = 8.0;
/// The least the volume name keeps before the free space gives way too.
const NAME_MIN: f32 = 64.0;
/// How much wider the volume name's semibold runs than the estimate of the regular face.
const STRONG: f32 = 1.08;

/// What the drive line shows besides the drive and its free space; both on until read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSettings {
    /// `panels.drive_line_volume`: the volume name in brackets.
    pub volume: bool,
    /// `panels.drive_line_buttons`: `\` and `..` at the line's end.
    pub buttons: bool,
}

impl Default for LineSettings {
    fn default() -> Self {
        Self {
            volume: true,
            buttons: true,
        }
    }
}

/// A button at the drive line's end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStep {
    /// `\`: the drive's root folder.
    Root,
    /// `..`: one folder up.
    Up,
}

impl LineStep {
    /// The keymap row the button stands for.
    pub fn binding(self) -> &'static str {
        match self {
            Self::Root => "root_folder",
            Self::Up => "parent_folder",
        }
    }

    /// The command of that row, for `panel` (`None`: the active one, as a key runs it).
    pub fn command(self, panel: Option<PanelId>) -> Command {
        let path = match self {
            Self::Root => "/",
            Self::Up => "..",
        };
        Command::Navigate {
            panel,
            path: path.into(),
        }
    }

    fn face(self) -> &'static str {
        match self {
            Self::Root => "\\",
            Self::Up => "..",
        }
    }

    fn hint(self, keys: &str) -> String {
        match self {
            Self::Root => t!("drives.line_root", keys = keys),
            Self::Up => t!("drives.line_up", keys = keys),
        }
    }
}

/// Whether the setting `key` is one of the drive line's.
pub fn concerns(key: &str) -> bool {
    key == DRIVE_LINE_VOLUME_KEY || key == DRIVE_LINE_BUTTONS_KEY
}

/// The stored settings of the line; each on unless switched off.
pub async fn load(db: Db) -> LineSettings {
    LineSettings {
        volume: stored_bool(&db, DRIVE_LINE_VOLUME_KEY).await,
        buttons: stored_bool(&db, DRIVE_LINE_BUTTONS_KEY).await,
    }
}

/// What a click on `step` in `panel`'s line runs: the panel becomes active, then goes there - one
/// job, as a click on a drive.
pub(super) fn step_commands(step: LineStep, panel: PanelId, active: PanelId) -> Vec<Command> {
    let mut commands = Vec::with_capacity(2);
    if panel != active {
        commands.push(Command::SwitchActivePanel { panel: Some(panel) });
    }
    commands.push(step.command(Some(panel)));
    commands
}

/// Whether `\` and `..` lead anywhere: a folder above `path`, and the file list in view.
pub(super) fn can_step(path: &VfsPath, view: PanelView) -> bool {
    view != PanelView::Terminal && path.parent().is_some()
}

/// The keys of the keymap's rows named `name`, as the keyboard reference writes them.
pub(super) fn keys_of(keymap: &Keymap, name: &str) -> String {
    keymap
        .bindings()
        .iter()
        .filter(|binding| binding.name == name)
        .map(|binding| crate::keymap::label(&binding.chord))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The name in brackets after the drive: a network drive's folder or the volume name, a
/// server's or a distribution's name; none for a drive without one.
pub(super) fn volume_name(drive: &Drive) -> Option<&str> {
    match drive.group {
        DriveGroup::Wsl | DriveGroup::Servers => Some(drive.label.as_str()),
        DriveGroup::Drives => drive.remote.as_deref().or(drive.volume.as_deref()),
    }
    .filter(|name| !name.is_empty())
}

/// The bracketed name and the free space in `width`: unchanged when both fit. Otherwise the name
/// gives way first, down to [`NAME_MIN`], then the free space too; each is shortened in the
/// middle to its share.
pub(super) fn fit_texts(name: Option<&str>, info: &str, width: f32) -> (Option<String>, String) {
    let strong = |part: &str| estimate_width(part, SMALL) * STRONG;
    let brackets = strong("[]");
    let name_width = name.map_or(0.0, |name| strong(name) + brackets);
    let gap = if name.is_some() && !info.is_empty() {
        GAP
    } else {
        0.0
    };
    let room = (width - gap).max(0.0);
    let info_room = estimate_width(info, SMALL)
        .min(room - name_width.min(NAME_MIN))
        .max(0.0);
    let name = name.map(|name| {
        let shown = middle_ellipsis_by(name, room - info_room - brackets, strong);
        format!("[{shown}]")
    });
    (name, middle_ellipsis(info, info_room, SMALL).into_owned())
}

/// `side`'s drive line.
pub(super) fn drive_line<'a>(
    app: &'a App,
    side: PanelId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let active = side == app.active;
    let settings = app.drive_bar.line;
    let drive = app.current_drive(side);
    let name = drive
        .filter(|_| settings.volume)
        .and_then(volume_name)
        .map(str::to_owned);
    let info = drive.map(line_text).unwrap_or_default();
    let fitted = responsive(move |size| texts(name.as_deref(), &info, size.width, tokens))
        .width(Length::Fill)
        .height(Length::Shrink);
    let mut line = row![drive_button(drive, side, tokens), fitted]
        .spacing(GAP)
        .align_y(Alignment::Center);
    if settings.buttons {
        let pane = &app.pane(side).state;
        let enabled = can_step(&pane.path, pane.view);
        let steps = [LineStep::Root, LineStep::Up].map(|step| {
            let hint = step.hint(&keys_of(&app.keymap, step.binding()));
            step_button(step, side, enabled, hint, tokens)
        });
        line = line.push(Row::with_children(steps).spacing(SPACING));
    }
    let body = container(line)
        .padding([2, 6])
        .width(Length::Fill)
        .style(move |_| theme::drive_line(tokens, active));
    let rule = container(Space::new().width(Length::Fill).height(1))
        .style(move |_| theme::drive_line_rule(tokens, active));
    column![body, rule].into()
}

/// The drive: its symbol, its letter or name and the arrow of the menu it opens.
fn drive_button<'a>(
    drive: Option<&'a Drive>,
    side: PanelId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (symbol, label) = drive.map_or(("\u{25ab}", ""), |drive| {
        (kind_symbol(drive.kind), short_label(drive))
    });
    let face = container(
        row![small(symbol), small(label), small(FOLDER_ARROW)]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .padding([1, 7])
    .style(move |_| button_style(tokens, false));
    mouse_area(face)
        .on_press(Message::Drive(DriveMessage::OpenMenu(side)))
        .interaction(Interaction::Pointer)
        .into()
}

/// The name in bold and the free space dimmed, fitted to `width`; the whole line in a tooltip
/// only when either was shortened.
fn texts<'a>(
    name: Option<&str>,
    info: &str,
    width: f32,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (shown_name, shown_info) = fit_texts(name, info, width);
    let whole = match name {
        Some(name) => format!("[{name}]  {info}"),
        None => info.to_owned(),
    };
    let shortened =
        shown_info != info || name.is_some_and(|name| shown_name != Some(format!("[{name}]")));
    let mut parts = row![].spacing(GAP).align_y(Alignment::Center);
    if let Some(shown) = shown_name {
        parts = parts.push(small(shown).font(theme::UI_FONT_STRONG));
    }
    parts = parts.push(small(shown_info).color(tokens.text_secondary));
    let fitted = container(parts).width(Length::Fill).clip(true);
    if shortened {
        theme::fit::tip(fitted, whole, tokens)
    } else {
        fitted.into()
    }
}

/// `\` or `..`: a small button, dimmed and without action while `enabled` is false.
fn step_button<'a>(
    step: LineStep,
    side: PanelId,
    enabled: bool,
    hint: String,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut label = small(step.face());
    if !enabled {
        label = label.color(tokens.text_secondary);
    }
    let face = container(label)
        .padding([1, 7])
        .style(move |_| button_style(tokens, false));
    let button: Element<'a, Message> = if enabled {
        mouse_area(face)
            .on_press(Message::Drive(DriveMessage::Step(side, step)))
            .interaction(Interaction::Pointer)
            .into()
    } else {
        face.into()
    };
    with_tooltip(button, hint, tokens)
}

#[cfg(test)]
#[path = "line_tests.rs"]
mod tests;
