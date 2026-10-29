//! The dialog's keyboard: everything works without the mouse.
//!
//! Enter starts, Escape closes without a change, F2 saves the preset. Tab and Shift+Tab walk the
//! dialog's focus ring - every field, switch, list and button (RC-080-16); the cursor keys move in
//! the table, Ctrl+ArrowUp and Ctrl+ArrowDown move the row
//! under the cursor (the counter follows the order). The labels name the Alt keys: Alt+N name
//! mask, Alt+X extension mask, Alt+S search, Alt+R replace, Alt+T counter, Alt+P preset name,
//! Alt+A `^`, Alt+1 `1x`, Alt+E `[E]`, Alt+G RegEx, Alt+B Subst., Alt+C the next case,
//! Alt+D `<Default>`, Alt+O sort by name, Alt+L the log, Alt+U undo.

use iced::widget::{Id, operation};
use rc_engine::Chord;

use super::{Field, Flag, MrMsg, PresetChoice};
use crate::app::{App, Effect};

/// Gives the text field `field` the keys.
pub(super) fn focus(field: Field) -> Effect {
    Effect::Task(operation::focus(Id::new(field.id())))
}

impl App {
    /// A key while the dialog is open, whether a text field used it or not: the dialog's keys
    /// are none a text field types.
    pub(super) fn multi_rename_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        let key = chord.key.as_str();
        let plain = chord.is_plain();
        if chord.alt && !chord.ctrl && !chord.shift {
            return self.multi_rename_alt(key);
        }
        if chord.ctrl && !chord.alt && !chord.shift {
            return match key {
                "ArrowUp" => self.on_multi_rename(MrMsg::MoveUp),
                "ArrowDown" => self.on_multi_rename(MrMsg::MoveDown),
                _ => Vec::new(),
            };
        }
        if !plain {
            return Vec::new();
        }
        let page = dialog.list.page() as isize - 1;
        match key {
            "Escape" => return self.on_multi_rename(MrMsg::Close),
            "Enter" => return self.on_multi_rename(MrMsg::Start),
            "F2" => return self.on_multi_rename(MrMsg::SavePreset),
            "ArrowUp" => dialog.list.step(-1),
            "ArrowDown" => dialog.list.step(1),
            "PageUp" => dialog.list.step(-page),
            "PageDown" => dialog.list.step(page),
            _ => {}
        }
        Vec::new()
    }

    /// The Alt keys of the labels.
    fn multi_rename_alt(&mut self, key: &str) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        let field = match key {
            "n" => Some(Field::NameMask),
            "x" => Some(Field::ExtensionMask),
            "s" => Some(Field::Find),
            "r" => Some(Field::Replace),
            "t" => Some(Field::CounterStart),
            "p" => Some(Field::PresetName),
            _ => None,
        };
        if let Some(field) = field {
            return vec![focus(field)];
        }
        let flag = match key {
            "a" => Some(Flag::CaseSensitive),
            "1" => Some(Flag::FirstOnly),
            "e" => Some(Flag::WithExtension),
            "g" => Some(Flag::RegEx),
            "b" => Some(Flag::Substitute),
            _ => None,
        };
        if let Some(flag) = flag {
            let on = !*dialog.form.flag_mut(flag);
            return self.on_multi_rename(MrMsg::Flag(flag, on));
        }
        let message = match key {
            "c" => MrMsg::Case(dialog.form.case.next()),
            "d" => MrMsg::Preset(PresetChoice::Default),
            "o" => MrMsg::SortByName,
            "l" => MrMsg::Log(!dialog.log),
            "u" => MrMsg::Undo,
            _ => return Vec::new(),
        };
        self.on_multi_rename(message)
    }
}
