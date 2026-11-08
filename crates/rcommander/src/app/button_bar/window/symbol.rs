//! "Choose symbol..." of the button bars' window (RC-110-10): the symbol picker
//! (`app/symbol_picker`) over the form of the button at the cursor; OK puts the symbol it took
//! into the button as `rc-symbol:<name>`, index 0, Cancel and Escape leave the button as it was.

use iced::widget::{Id, operation};
use rc_engine::BarItem;

use super::{NAME_FIELD, WinMsg, win};
use crate::app::{
    App, Effect, Message,
    symbol_picker::{self, Outcome as Picked, PickerMsg},
};

/// A message of the symbol picker as the window's.
pub(super) fn picker_message(message: PickerMsg) -> Message {
    win(WinMsg::Picker(message))
}

/// The stop of the picker's ring that has the keys after a walk.
pub(super) fn moved(id: Option<Id>) -> Message {
    picker_message(PickerMsg::Focused(id))
}

impl App {
    /// The keys while the picker is open over the window: its own (`symbol_picker::key`).
    pub(super) fn bar_picker_keys(&mut self, message: &Message) -> Option<Vec<Effect>> {
        let focus = self.button_bar.window.as_ref()?.picker.as_ref()?.focus;
        let action = match message {
            Message::Key(chord) => symbol_picker::key(focus, chord, false),
            Message::KeyCaptured(chord) => symbol_picker::key(focus, chord, true),
            Message::Press => return Some(Vec::new()),
            _ => return None,
        };
        Some(symbol_picker::effects(action, moved, |message| {
            self.on_bar_window(WinMsg::Picker(message))
        }))
    }

    /// "Choose symbol...": the picker over the form, on the button's symbol (RC-110-10).
    pub(super) fn open_button_symbol_picker(&mut self) -> Vec<Effect> {
        let Some(button) = self
            .button_bar
            .window
            .as_ref()
            .and_then(|window| window.items.get(window.cursor?))
            .and_then(BarItem::button)
        else {
            return Vec::new();
        };
        let current = button.icon.as_ref().map_or("", |icon| icon.path.as_str());
        let picker = self.symbol_picker(current, &button.tooltip);
        let follow = symbol_picker::follow(&picker);
        if let Some(window) = self.button_bar.window.as_mut() {
            window.picker = Some(picker);
        }
        vec![symbol_picker::focus_search(), follow]
    }

    /// A message of the picker: the symbol taken into the button, or the form back as it was.
    pub(super) fn on_button_symbol_picker(&mut self, message: PickerMsg) -> Vec<Effect> {
        let Some(window) = self.button_bar.window.as_mut() else {
            return Vec::new();
        };
        let Some(picker) = window.picker.as_mut() else {
            return Vec::new();
        };
        match picker.update(message) {
            Picked::Open(true) => return vec![symbol_picker::follow(picker)],
            Picked::Open(false) => return Vec::new(),
            Picked::Chosen(path) => {
                if let Some(button) = window.button_mut() {
                    button.icon = Some(rc_engine::IconRef { path, index: 0 });
                    window.dirty = true;
                }
            }
            Picked::Cancelled => {}
        }
        window.picker = None;
        vec![Effect::Task(operation::focus(Id::new(NAME_FIELD)))]
    }
}
