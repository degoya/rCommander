//! The viewer's text size (RC-020-12): Ctrl+wheel, Ctrl++ and Ctrl+- make the text of Lister and
//! Quick View larger and smaller in steps, Ctrl+0 takes it back to the size of the settings; in
//! the image view the same keys zoom the picture as `+` and `-` do, and Ctrl+0 fits it.
//!
//! A size belongs to its window: the Lister and the files N and P step to, or Quick View while
//! its cursor moves on. With `viewer.text_zoom_keep` at `setting` it is stored as
//! `viewer.font_size` too, so every viewer opens at it; Ctrl+0 then stores the default 13. With
//! `viewer.text_zoom` off the keys are given back: Ctrl+wheel scrolls, and in Quick View
//! Ctrl+NumAdd and Ctrl+NumSubtract select and deselect all again.
//!
//! A text view hears the wheel in its editor, which names the window under the pointer. The
//! picture and the Markdown preview lie in a scrollable that keeps the wheel for itself, so a
//! subscription hears it while Ctrl is held ([`wheel_events`]), for the viewer that has the
//! keyboard. The top line stays: the window counts the lines that fit again and the page is read
//! from the same byte; the preview keeps its share of the way down.

use iced::{
    Task,
    event::{self, Event},
    mouse::{self, ScrollDelta},
    widget::{operation, scrollable::RelativeOffset, text_editor::Action},
    window,
};
use rc_db::Db;
use rc_engine::{
    Command,
    settings::{
        DEFAULT_FONT_SIZE, TEXT_ZOOM_KEEP_KEY, TEXT_ZOOM_KEY, VIEWER_FONT_SIZE_KEY,
        VIEWER_MAX_FONT_SIZE, VIEWER_MIN_FONT_SIZE, stored_bool, stored_setting,
    },
    viewer::{TextEncoding, ViewMode},
};
use serde_json::Value;

use super::{
    ViewerMessage,
    keys::ListerKey,
    lister::{Lister, Zoom},
    markdown::scroll_id,
    quick::QuickShown,
    wrap,
};
use crate::{
    app::{App, Effect, Message, dialog_windows::DialogWindow},
    i18n::t,
    theme::{self, fonts::AreaFont},
};

/// The sizes the keys step through, in points: one apart up to 16, wider above.
const STEPS: [i64; 26] = [
    6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 26, 28, 32, 36, 40, 44, 48, 54, 60, 66,
    72,
];

/// What the settings make of the keys: on or off, and whether a size is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextZoom {
    pub enabled: bool,
    /// `viewer.text_zoom_keep` is `setting`: a size is stored as `viewer.font_size`.
    pub save: bool,
}

impl Default for TextZoom {
    fn default() -> Self {
        Self {
            enabled: true,
            save: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TextSizeMsg {
    /// The two settings, read at the start and after one changed.
    Settings(TextZoom),
    /// The wheel turned in `window` while Ctrl was held; above 0 away from the person.
    Wheel(window::Id, f32),
}

/// Whether a changed setting is one of the text size's.
pub fn concerns(key: &str) -> bool {
    key == TEXT_ZOOM_KEY || key == TEXT_ZOOM_KEEP_KEY
}

/// The size a step from `size` larger or smaller; the ends of 6 and 72 hold.
pub fn step(size: f32, larger: bool) -> f32 {
    let next = if larger {
        STEPS.iter().find(|step| **step as f32 > size + 0.5)
    } else {
        STEPS.iter().rev().find(|step| (**step as f32) < size - 0.5)
    };
    let end = if larger {
        VIEWER_MAX_FONT_SIZE
    } else {
        VIEWER_MIN_FONT_SIZE
    };
    next.copied().unwrap_or(end) as f32
}

/// The wheel, for the subscription that runs while Ctrl is held ([`App::wheel_sizes_text`]).
pub fn wheel_events(event: Event, _status: event::Status, window: window::Id) -> Option<Message> {
    let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event else {
        return None;
    };
    let (ScrollDelta::Lines { y, .. } | ScrollDelta::Pixels { y, .. }) = delta;
    Some(wrap(ViewerMessage::TextSize(TextSizeMsg::Wheel(window, y))))
}

/// Reads `viewer.text_zoom` and `viewer.text_zoom_keep`; their defaults without them.
pub async fn load(db: Db) -> TextZoom {
    let enabled = stored_bool(&db, TEXT_ZOOM_KEY).await;
    let save = stored_setting(&db, TEXT_ZOOM_KEEP_KEY)
        .await
        .is_some_and(|value| value.as_str() == Some("setting"));
    TextZoom { enabled, save }
}

impl Lister {
    /// The viewer's font at this window's own size, while it has one.
    pub fn font(&self) -> AreaFont {
        let font = theme::fonts::fonts().viewer;
        AreaFont {
            size: self.text_size.unwrap_or(font.size),
            ..font
        }
    }

    /// A text line's height: DOS text (CP437) as tall as the font's full block, floored so the
    /// blocks of its art touch rather than leave a seam (`viewer.dos_tight_lines`); any other
    /// text, and every other mode, at the font's own.
    pub fn line_height(&self) -> f32 {
        let font = self.font();
        match theme::fonts::fonts().dos_block {
            Some(block)
                if self.mode == ViewMode::Text && self.detected.encoding == TextEncoding::Cp437 =>
            {
                tight_line(font.size, block)
            }
            _ => font.line_height(),
        }
    }
}

/// The height of a DOS line at `size` in a font whose full block is `block` times its size:
/// floored, so neighbouring blocks overlap by a fraction of a pixel instead of leaving a seam.
fn tight_line(size: f32, block: f32) -> f32 {
    (size * block).floor().max(1.0)
}

impl App {
    /// Reads the two settings (the start, and after one changed).
    pub(in crate::app) fn text_zoom_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(load(db), |zoom| {
            wrap(ViewerMessage::TextSize(TextSizeMsg::Settings(zoom)))
        })
    }

    /// Whether the wheel is heard: the keys are on, Ctrl is held and a viewer shows a file.
    pub(in crate::app) fn wheel_sizes_text(&self) -> bool {
        let quick_file = self
            .viewers
            .quick
            .as_ref()
            .is_some_and(|quick| matches!(quick.shown, QuickShown::File(_)));
        self.viewers.text_zoom.enabled
            && self.modifiers_held().ctrl
            && (quick_file || !self.viewers.open.is_empty())
    }

    pub(super) fn on_text_size(&mut self, message: TextSizeMsg) -> Vec<Effect> {
        match message {
            TextSizeMsg::Settings(zoom) => {
                self.viewers.text_zoom = zoom;
                Vec::new()
            }
            TextSizeMsg::Wheel(window, y) => {
                let Some(id) = self.wheel_target(window) else {
                    return Vec::new();
                };
                // A text view hears the wheel in its editor ([`App::editor_sizes_text`]).
                let scrolls_itself = self.viewers.lister(id).is_some_and(|lister| {
                    lister.mode == ViewMode::Image || lister.preview().is_some()
                });
                let key = match y {
                    _ if !scrolls_itself => return Vec::new(),
                    y if y > 0.0 => ListerKey::TextLarger,
                    y if y < 0.0 => ListerKey::TextSmaller,
                    _ => return Vec::new(),
                };
                self.text_size_key(id, key).unwrap_or_default()
            }
        }
    }

    /// Ctrl+wheel in a text view's editor: that window's text is sized instead of scrolled.
    pub(super) fn editor_sizes_text(&mut self, message: &ViewerMessage) -> Option<Vec<Effect>> {
        let ViewerMessage::Editor(id, Action::Scroll { lines }) = message else {
            return None;
        };
        if !self.viewers.text_zoom.enabled || !self.modifiers_held().ctrl {
            return None;
        }
        let key = match lines.signum() {
            -1 => ListerKey::TextLarger,
            1 => ListerKey::TextSmaller,
            _ => return Some(Vec::new()),
        };
        Some(self.text_size_key(*id, key).unwrap_or_default())
    }

    /// Ctrl++, Ctrl+- or Ctrl+0 for the viewer `id`; `None` for any other key, and for these
    /// while the setting has them off.
    pub(super) fn text_size_key(&mut self, id: u64, key: ListerKey) -> Option<Vec<Effect>> {
        if !key.sizes_text() || !self.viewers.text_zoom.enabled {
            return None;
        }
        let save = self.viewers.text_zoom.save;
        let lister = self.viewers.lister_mut(id)?;
        if lister.mode == ViewMode::Image {
            zoom_picture(lister, key);
            return Some(Vec::new());
        }
        let size = match key {
            ListerKey::TextLarger => step(lister.font().size, true),
            ListerKey::TextSmaller => step(lister.font().size, false),
            _ if save => DEFAULT_FONT_SIZE as f32,
            _ => theme::fonts::fonts().viewer.size,
        };
        // Ctrl+0 in a window of its own size gives it the size of the settings back.
        lister.text_size = (save || key != ListerKey::TextReset).then_some(size);
        lister.note = Some(if save {
            t!("viewer.text_size_saved", size = size)
        } else {
            t!("viewer.text_size", size = size)
        });
        let mut effects = Vec::new();
        if lister.preview().is_some() {
            let at = RelativeOffset {
                x: None,
                y: Some(lister.preview_at),
            };
            effects.push(Effect::Task(operation::snap_to(scroll_id(id), at)));
        }
        let text_size = lister.text_size;
        if let Some(quick) = &mut self.viewers.quick
            && quick.shows(id)
        {
            quick.text_size = text_size;
        }
        if save {
            let set = Command::SetSetting {
                key: VIEWER_FONT_SIZE_KEY.to_owned(),
                value: Value::from(size as i64),
            };
            effects.push(Effect::Dispatch(self.active, vec![set]));
        }
        Some(effects)
    }

    /// The viewer the wheel in `window` sizes: a Lister's own window, else the Lister over the
    /// main window, else Quick View while its panel is the active one.
    fn wheel_target(&self, window: window::Id) -> Option<u64> {
        if let Some(DialogWindow::Lister(id)) = self.dialog_windows.kind_of(window) {
            return Some(id);
        }
        if self.lister_over_main() {
            return self.viewers.open.last().map(|lister| lister.id);
        }
        let quick = self.viewers.quick.as_ref()?;
        match &quick.shown {
            QuickShown::File(lister) if quick.target() == self.active => Some(lister.id),
            _ => None,
        }
    }
}

/// The image view: Ctrl++ and Ctrl+- zoom as `+` and `-` do, Ctrl+0 fits the picture again.
fn zoom_picture(lister: &mut Lister, key: ListerKey) {
    match key {
        ListerKey::TextLarger => {
            lister.key(ListerKey::ZoomIn);
        }
        ListerKey::TextSmaller => {
            lister.key(ListerKey::ZoomOut);
        }
        _ => {
            lister.zoom = Zoom {
                turns: lister.zoom.turns,
                ..Zoom::default()
            };
        }
    }
}

#[cfg(test)]
#[path = "text_size_tests.rs"]
mod tests;
