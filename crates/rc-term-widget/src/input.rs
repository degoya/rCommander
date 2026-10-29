//! Input as the program gets it, worked out without a window: keys through the bindings,
//! pastes, mouse reports and the wheel (from iced_term 0.8's `backend.rs` and `view.rs`, MIT),
//! and pixels to grid cells.

use alacritty_terminal::{
    event::EventListener,
    index::{Column, Point as GridPoint, Side},
    term::{Term, TermMode, viewport_to_point},
};
use iced::{
    Point, Size,
    keyboard::{Key, Modifiers},
    mouse::ScrollDelta,
};
use rc_term::TermSize;

use crate::bindings::{BindingAction, BindingsLayout, InputKind};

/// What a key press asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyInput {
    /// Bytes for the program.
    Write(Vec<u8>),
    /// The selection to the clipboard.
    Copy,
    /// The clipboard to the program.
    Paste,
    /// The history a page up or down.
    PageUp,
    PageDown,
    /// Nothing for the terminal.
    None,
}

/// Whether `key` with `modifiers` is a plain Ctrl+C, which copies a selection when
/// [`crate::CopySettings::ctrl_c`] is on.
pub(crate) fn is_ctrl_c(key: &Key, modifiers: Modifiers) -> bool {
    is_plain_ctrl(key, modifiers, "c")
}

/// Whether `key` with `modifiers` is a plain Ctrl+V, which pastes when
/// [`crate::CopySettings::ctrl_v`] is on.
pub(crate) fn is_ctrl_v(key: &Key, modifiers: Modifiers) -> bool {
    is_plain_ctrl(key, modifiers, "v")
}

fn is_plain_ctrl(key: &Key, modifiers: Modifiers, letter: &str) -> bool {
    matches!(key, Key::Character(character) if character.to_lowercase() == letter)
        && modifiers == Modifiers::CTRL
}

/// Whether the mouse goes to the program in `mode`: Shift keeps it for selecting when
/// `shift_selects`, as Windows Terminal does.
pub(crate) fn mouse_to_program(mode: TermMode, modifiers: Modifiers, shift_selects: bool) -> bool {
    mode.intersects(TermMode::MOUSE_MODE) && !(shift_selects && modifiers.shift())
}

/// The text selected in `term`, when any is; `clear` drops the selection after, as a copy by
/// Ctrl+C or a right click does.
pub(crate) fn take_selection<T: EventListener>(term: &mut Term<T>, clear: bool) -> Option<String> {
    let text = term.selection_to_string().filter(|text| !text.is_empty())?;
    if clear {
        term.selection = None;
    }
    Some(text)
}

/// What `key` with `modifiers` does in a terminal in `mode`; `text` is what the key types.
///
/// A bound key sends its sequence. Any other key sends the text it types, with Alt (but not
/// AltGr, which arrives as Ctrl+Alt) as an escape prefix, as xterm's `metaSendsEscape` does.
pub(crate) fn key_input(
    bindings: &BindingsLayout,
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
    mode: TermMode,
) -> KeyInput {
    let input = match key {
        Key::Character(character) => InputKind::Char(character.to_lowercase()),
        Key::Named(named) => InputKind::KeyCode(*named),
        _ => return KeyInput::None,
    };
    match bindings.action(&input, modifiers, mode) {
        BindingAction::Char(c) => {
            let mut buffer = [0; 4];
            KeyInput::Write(c.encode_utf8(&mut buffer).as_bytes().to_vec())
        }
        BindingAction::Esc(sequence) => KeyInput::Write(sequence.into_bytes()),
        BindingAction::Copy => KeyInput::Copy,
        BindingAction::Paste => KeyInput::Paste,
        BindingAction::ScrollPageUp => KeyInput::PageUp,
        BindingAction::ScrollPageDown => KeyInput::PageDown,
        BindingAction::Ignore => match text.filter(|text| !text.is_empty()) {
            Some(text) if modifiers.alt() && !modifiers.control() => {
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice(text.as_bytes());
                KeyInput::Write(bytes)
            }
            Some(text) => KeyInput::Write(text.as_bytes().to_vec()),
            None => KeyInput::None,
        },
    }
}

/// Whether pasting `text` would do more than type it: a line break runs what comes before it,
/// any other control character but a tab acts as its key would - Ctrl+C (`\x03`) ends the
/// program, Ctrl+U (`\x15`) and Ctrl+W (`\x17`) wipe the line, a backspace or an escape edits
/// it (RC-080-51).
pub(crate) fn acts_beyond_typing(text: &str) -> bool {
    text.chars().any(|c| c.is_control() && c != '\t')
}

/// Pasted text as the program gets it: between bracketed-paste markers when the program asked
/// for them (escape characters removed, so the text cannot end the bracket), line breaks as
/// Enter otherwise.
pub(crate) fn paste_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// The grid that fits `size` with cells of `cell`; `None` below one row or column.
pub(crate) fn grid_size(size: Size, cell: Size) -> Option<TermSize> {
    let rows = (size.height / cell.height).floor();
    let cols = (size.width / cell.width).floor();
    if rows < 1.0 || cols < 1.0 {
        return None;
    }
    let max = f32::from(u16::MAX);
    TermSize::new(rows.min(max) as u16, cols.min(max) as u16).ok()
}

/// The cell under `position` (relative to the widget), in a grid of `columns` by `lines`
/// scrolled `display_offset` lines into its history.
pub(crate) fn grid_point(
    position: Point,
    cell: Size,
    columns: usize,
    lines: usize,
    display_offset: usize,
) -> GridPoint {
    let column = ((position.x.max(0.0) / cell.width) as usize).min(columns.saturating_sub(1));
    let line = ((position.y.max(0.0) / cell.height) as usize).min(lines.saturating_sub(1));
    viewport_to_point(display_offset, GridPoint::new(line, Column(column)))
}

/// Which half of its cell `x` is in, for where a selection starts or ends.
pub(crate) fn side(x: f32, cell: Size) -> Side {
    if x.max(0.0) % cell.width > cell.width / 2.0 {
        Side::Right
    } else {
        Side::Left
    }
}

/// The left button, pressed or released, and moved while held, as xterm numbers them.
pub(crate) const LEFT_BUTTON: u8 = 0;
pub(crate) const LEFT_MOVE: u8 = 32;
/// What X10 reports for a release.
const RELEASE: u8 = 3;

/// A mouse report for a program that asked for the mouse (`mode` has a mouse mode): SGR when
/// it asked for that, X10 (UTF-8 extended or not) otherwise; `None` where X10 cannot say the
/// cell.
pub(crate) fn mouse_report(
    button: u8,
    modifiers: Modifiers,
    point: GridPoint,
    pressed: bool,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let mut held = 0;
    if modifiers.shift() {
        held += 4;
    }
    if modifiers.alt() {
        held += 8;
    }
    if modifiers.control() {
        held += 16;
    }
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        let report = format!(
            "\x1b[<{};{};{}{end}",
            button + held,
            point.column.0 + 1,
            point.line.0 + 1
        );
        return Some(report.into_bytes());
    }
    let base = if pressed { button } else { RELEASE };
    let code = base + held;
    x10_report(point, code, mode.contains(TermMode::UTF8_MOUSE))
}

fn x10_report(point: GridPoint, code: u8, utf8: bool) -> Option<Vec<u8>> {
    let line = usize::try_from(point.line.0).ok()?;
    let column = point.column.0;
    let max = if utf8 { 2015 } else { 223 };
    if line >= max || column >= max {
        return None;
    }
    let mut report = vec![0x1b, b'[', b'M', 32 + code];
    for position in [column, line] {
        let value = 32 + 1 + position;
        if utf8 && position >= 95 {
            report.extend([(0xC0 + value / 64) as u8, (0x80 + (value & 63)) as u8]);
        } else {
            report.push(value as u8);
        }
    }
    Some(report)
}

/// Lines a wheel event scrolls, positive into the history; pixels below a line are kept in
/// `pixels` for the next event. Three lines a notch, as the file list.
pub(crate) fn wheel_lines(delta: ScrollDelta, pixels: &mut f32, line_height: f32) -> i32 {
    match delta {
        ScrollDelta::Lines { y, .. } => (y * 3.0).round() as i32,
        ScrollDelta::Pixels { y, .. } => {
            *pixels += y;
            let lines = (*pixels / line_height).trunc();
            *pixels %= line_height;
            lines as i32
        }
    }
}

/// Cursor-key presses for a full-screen program that scrolls with the arrows (alternate
/// scroll): up for positive `lines`.
pub(crate) fn alternate_scroll(lines: i32) -> Vec<u8> {
    let key = if lines > 0 { b'A' } else { b'B' };
    (0..lines.unsigned_abs())
        .flat_map(|_| [0x1b, b'O', key])
        .collect()
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
