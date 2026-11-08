//! A setting of one entry per line - the ignore list (RC-100-92, `SettingKind::MaskLines`) - as a
//! column of one-line fields: each line its own field, an empty one below for the next. A line
//! emptied goes; the engine trims the lines and drops blank ones when the setting is stored.

use iced::{
    Element, Length,
    widget::{Column, text_input},
};

use super::{SettingsMessage, wrap};
use crate::app::Message;

/// The fields of the setting `key`, whose value is `text`.
pub fn lines<'a>(key: &'static str, text: &str) -> Element<'a, Message> {
    let mut shown: Vec<String> = text
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    shown.push(String::new());
    let fields = shown.iter().enumerate().map(|(at, line)| {
        let all = shown.clone();
        text_input("", line)
            .on_input(move |typed| wrap(SettingsMessage::Edit(key, joined(&all, at, &typed))))
            .width(Length::Fill)
            .into()
    });
    Column::with_children(fields).spacing(4).into()
}

/// `lines` with line `at` as `typed`: the value the setting takes, empty lines dropped.
fn joined(lines: &[String], at: usize, typed: &str) -> String {
    let mut lines = lines.to_vec();
    match lines.get_mut(at) {
        Some(line) => typed.clone_into(line),
        None => lines.push(typed.to_owned()),
    }
    lines.retain(|line| !line.is_empty());
    lines.join("\n")
}

#[cfg(test)]
#[path = "lines_field_tests.rs"]
mod tests;
