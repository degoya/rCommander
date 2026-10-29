//! The path bar above a panel's list: the directory as clickable segments, each going there, and
//! beside them a free space whose click turns the bar into a text field for a path to go to.
//! Inside an archive the segments run through the archive file (`archive_view.rs`). A directory
//! on a server starts with its location, `sftp://<connection name>/` or `ftp://<connection
//! name>/` (RC-070-03), and is separated by `/` on every platform. The segments are in the
//! file list's chosen font (RC-010-17).

use std::path::{Component, Path, PathBuf};

use iced::{
    Element, Length,
    widget::{Space, column, container, mouse_area, row, text, text_input},
};
use rc_engine::{FTP_SCHEME, PanelId, SFTP_SCHEME, VfsPath};
use rc_vfs::BackendId;

use super::{
    App, Message,
    file_list::divider,
    keys::{PROMPT_ID, PromptKind},
};
use crate::theme::{self, Tokens};

/// The segments of `path`, each with the directory it stands for: the root (`C:\`, `/`,
/// `\\server\share\`) and then one per directory.
pub fn breadcrumb(path: &Path) -> Vec<(String, PathBuf)> {
    let mut so_far = PathBuf::new();
    let mut segments = Vec::new();
    for component in path.components() {
        so_far.push(component);
        match component {
            // A drive or share is shown with the root that follows it.
            Component::Prefix(_) | Component::CurDir | Component::ParentDir => {}
            Component::RootDir => segments.push((so_far.display().to_string(), so_far.clone())),
            Component::Normal(name) => {
                segments.push((name.to_string_lossy().into_owned(), so_far.clone()));
            }
        }
    }
    if segments.is_empty() && !so_far.as_os_str().is_empty() {
        segments.push((so_far.display().to_string(), so_far));
    }
    segments
}

/// The segments of the server path `path` after `prefix` (`sftp://<name>`): the prefix stands
/// for `/`, then one segment per directory.
pub fn remote_breadcrumb(prefix: &str, path: &Path) -> Vec<(String, PathBuf)> {
    let mut so_far = PathBuf::from("/");
    let mut segments = vec![(format!("{prefix}/"), so_far.clone())];
    for component in path.components() {
        if let Component::Normal(name) = component {
            so_far.push(name);
            segments.push((name.to_string_lossy().into_owned(), so_far.clone()));
        }
    }
    segments
}

impl App {
    /// A panel's directory as text: the path here, `sftp://<name>/<path>` or
    /// `ftp://<name>/<path>` on a server - what the path prompt starts with and takes back.
    pub(super) fn location_text(&self, path: &VfsPath) -> String {
        match self.remote_prefix(path) {
            Some(prefix) => {
                let names: Vec<String> = remote_breadcrumb("", &path.path)
                    .into_iter()
                    .skip(1)
                    .map(|(name, _)| name)
                    .collect();
                format!("{prefix}/{}", names.join("/"))
            }
            None => path.path.display().to_string(),
        }
    }

    /// `sftp://<connection name>` or `ftp://<connection name>` for a location on a server, by the
    /// drive bar's name for it; `None` on this machine.
    pub(super) fn remote_prefix(&self, path: &VfsPath) -> Option<String> {
        let (scheme, connection_id) = match path.backend {
            BackendId::Sftp { connection_id } => (SFTP_SCHEME, connection_id),
            BackendId::Ftp { connection_id } => (FTP_SCHEME, connection_id),
            _ => return None,
        };
        let id = format!("server:{connection_id}");
        let name = self
            .drive_bar
            .drives
            .iter()
            .find(|drive| drive.id == id)
            .map_or_else(|| connection_id.to_string(), |drive| drive.label.clone());
        Some(format!("{scheme}{name}"))
    }

    /// The path bar of `side`, or the path prompt while it asks.
    pub(super) fn path_bar(&self, side: PanelId, tokens: &'static Tokens) -> Element<'_, Message> {
        let active = side == self.active;
        let bar: Element<'_, Message> = match &self.prompt {
            Some(prompt) if prompt.panel == side && prompt.kind == PromptKind::Path => {
                prompt_field(crate::i18n::t!("prompt.path"), &prompt.text)
            }
            _ => {
                let state = &self.pane(side).state;
                let path = &state.path;
                let crumbs = match self.remote_prefix(path) {
                    Some(prefix) => (remote_breadcrumb(&prefix, &path.path), "/"),
                    None => (
                        super::archive_view::breadcrumb_of(path),
                        std::path::MAIN_SEPARATOR_STR,
                    ),
                };
                let segments = segments(crumbs, side, active, tokens);
                // A flat view (Ctrl+B, fed search hits) says so beside its folder.
                match &state.flat {
                    Some(flat) => row![segments, super::search::flat_badge(flat, tokens)]
                        .spacing(12)
                        .into(),
                    None => segments,
                }
            }
        };
        let bar = super::cmdline::with_filter(bar, &self.pane(side).state, tokens);
        column![
            container(bar)
                .padding([7, 12])
                .width(Length::Fill)
                .style(move |_| theme::header(tokens)),
            divider(tokens),
        ]
        .into()
    }
}

/// The segments and the separator between them.
type Crumbs = (Vec<(String, PathBuf)>, &'static str);

fn segments(
    (crumbs, separator): Crumbs,
    side: PanelId,
    active: bool,
    tokens: &'static Tokens,
) -> Element<'static, Message> {
    let list = theme::fonts::list();
    let (font, color) = look(list, active, tokens);
    let mut parts: Vec<Element<'static, Message>> = Vec::new();
    for (index, (label, target)) in crumbs.into_iter().enumerate() {
        // The root ends in a separator already; the others are joined by one.
        if index > 1 {
            parts.push(line(separator.to_owned(), list.size, font, color));
        }
        parts.push(
            mouse_area(line(label, list.size, font, color))
                .on_press(Message::PathSegment(side, target))
                .into(),
        );
    }
    parts.push(
        mouse_area(Space::new().width(Length::Fill).height(list.size))
            .on_press(Message::EditPath(side))
            .into(),
    );
    row(parts).into()
}

/// The segments' font and colour. Only the colour marks the active panel: a heavier weight the
/// chosen font lacks would be drawn in another font, and the path would jump.
fn look(list: theme::fonts::AreaFont, active: bool, tokens: &Tokens) -> (iced::Font, iced::Color) {
    let color = if active {
        tokens.accent
    } else {
        tokens.text_secondary
    };
    (list.font, color)
}

fn line(
    content: String,
    size: f32,
    font: iced::Font,
    color: iced::Color,
) -> Element<'static, Message> {
    text(content)
        .size(size)
        .font(font)
        .color(color)
        .wrapping(text::Wrapping::None)
        .into()
}

/// A prompt's label and text field: Enter submits, Escape (a key of the window) drops it.
pub(super) fn prompt_field<'a>(label: &'static str, value: &'a str) -> Element<'a, Message> {
    row![
        text(label).size(theme::TEXT_SIZE).font(theme::UI_FONT),
        text_input("", value)
            .id(PROMPT_ID)
            .on_input(Message::PromptInput)
            .on_submit(Message::PromptSubmit)
            .size(theme::TEXT_SIZE)
            .font(theme::UI_FONT)
            .padding([1, 4]),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

#[cfg(test)]
#[path = "path_bar_tests.rs"]
mod tests;
