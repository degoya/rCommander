//! What a key sends to the program: the escape sequences of xterm, by key, modifiers and the
//! terminal's mode. Taken from iced_term 0.8 (`bindings.rs`, MIT); Ctrl+U sends 0x15 (iced_term
//! sent `Q`), Ctrl+\ sends 0x1c, mouse and link bindings are gone, and Shift+PageUp/PageDown
//! scroll the history outside full-screen programs.

use alacritty_terminal::term::TermMode;
use iced::keyboard::{Modifiers, key::Named};

/// What a binding does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingAction {
    /// Copy the selection to the clipboard.
    Copy,
    /// Paste the clipboard.
    Paste,
    Char(char),
    Esc(String),
    /// Scroll the history a page up or down.
    ScrollPageUp,
    ScrollPageDown,
    Ignore,
}

/// The key a binding reacts to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InputKind {
    /// A character key, in lower case.
    Char(String),
    KeyCode(Named),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub target: InputKind,
    pub modifiers: Modifiers,
    /// Modes the terminal must be in.
    pub mode_include: TermMode,
    /// Modes the terminal must not be in.
    pub mode_exclude: TermMode,
}

/// `key[, modifiers][, +mode][, ~mode]; action` rows into bindings, as iced_term writes them.
macro_rules! bindings {
    (
        $(
            $input:tt
            $(,$modifiers:expr)*
            $(,+$include:expr)*
            $(,~$exclude:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        macro_rules! input_kind {
            ($key:ident) => {
                InputKind::KeyCode(Named::$key)
            };
            ($text:literal) => {
                InputKind::Char($text.to_owned())
            };
        }
        vec![$(
            (
                Binding {
                    target: input_kind!($input),
                    modifiers: Modifiers::empty() $(| $modifiers)*,
                    mode_include: TermMode::empty() $(| $include)*,
                    mode_exclude: TermMode::empty() $(| $exclude)*,
                },
                $action,
            ),
        )*]
    }};
}

/// The bindings in force, the first match winning.
#[derive(Clone, Debug)]
pub(crate) struct BindingsLayout {
    layout: Vec<(Binding, BindingAction)>,
}

impl Default for BindingsLayout {
    fn default() -> Self {
        let mut layout = Self {
            layout: keyboard_bindings(),
        };
        layout.add_bindings(platform_bindings());
        layout
    }
}

impl BindingsLayout {
    /// Adds `bindings`; one for a key, modifiers and modes already bound replaces that one.
    pub(crate) fn add_bindings(&mut self, bindings: Vec<(Binding, BindingAction)>) {
        for (binding, action) in bindings {
            match self.layout.iter().position(|(known, _)| *known == binding) {
                Some(position) => self.layout[position] = (binding, action),
                None => self.layout.push((binding, action)),
            }
        }
    }

    /// What `input` with `modifiers` does in `mode`.
    pub(crate) fn action(
        &self,
        input: &InputKind,
        modifiers: Modifiers,
        mode: TermMode,
    ) -> BindingAction {
        self.layout
            .iter()
            .find(|(binding, _)| {
                binding.target == *input
                    && binding.modifiers == modifiers
                    && mode.contains(binding.mode_include)
                    && !mode.intersects(binding.mode_exclude)
            })
            .map_or(BindingAction::Ignore, |(_, action)| action.clone())
    }
}

fn esc(sequence: &str) -> BindingAction {
    BindingAction::Esc(sequence.to_owned())
}

fn keyboard_bindings() -> Vec<(Binding, BindingAction)> {
    let mut rows = plain_bindings();
    rows.extend(modified_bindings());
    rows.extend(control_characters());
    rows
}

/// Keys without modifiers, and the cursor keys in both cursor modes.
fn plain_bindings() -> Vec<(Binding, BindingAction)> {
    use BindingAction::Char;
    bindings!(
        Space;     Char(' ');
        Enter;     Char('\x0d');
        Backspace; Char('\x7f');
        Escape;    Char('\x1b');
        Tab;       Char('\x09');
        Insert;    esc("\x1b[2~");
        Delete;    esc("\x1b[3~");
        PageUp;    esc("\x1b[5~");
        PageDown;  esc("\x1b[6~");
        F1;        esc("\x1bOP");
        F2;        esc("\x1bOQ");
        F3;        esc("\x1bOR");
        F4;        esc("\x1bOS");
        F5;        esc("\x1b[15~");
        F6;        esc("\x1b[17~");
        F7;        esc("\x1b[18~");
        F8;        esc("\x1b[19~");
        F9;        esc("\x1b[20~");
        F10;       esc("\x1b[21~");
        F11;       esc("\x1b[23~");
        F12;       esc("\x1b[24~");
        End,        ~TermMode::APP_CURSOR; esc("\x1b[F");
        Home,       ~TermMode::APP_CURSOR; esc("\x1b[H");
        ArrowUp,    ~TermMode::APP_CURSOR; esc("\x1b[A");
        ArrowDown,  ~TermMode::APP_CURSOR; esc("\x1b[B");
        ArrowLeft,  ~TermMode::APP_CURSOR; esc("\x1b[D");
        ArrowRight, ~TermMode::APP_CURSOR; esc("\x1b[C");
        End,        +TermMode::APP_CURSOR; esc("\x1bOF");
        Home,       +TermMode::APP_CURSOR; esc("\x1bOH");
        ArrowUp,    +TermMode::APP_CURSOR; esc("\x1bOA");
        ArrowDown,  +TermMode::APP_CURSOR; esc("\x1bOB");
        ArrowLeft,  +TermMode::APP_CURSOR; esc("\x1bOD");
        ArrowRight, +TermMode::APP_CURSOR; esc("\x1bOC");
    )
}

/// Named keys with Ctrl, Shift and Alt (xterm's `CSI 1;<m>` forms).
fn modified_bindings() -> Vec<(Binding, BindingAction)> {
    use BindingAction::Char;
    const CTRL: Modifiers = Modifiers::CTRL;
    const SHIFT: Modifiers = Modifiers::SHIFT;
    const ALT: Modifiers = Modifiers::ALT;
    let shift_alt = SHIFT | ALT;
    let shift_ctrl = SHIFT | CTRL;
    let ctrl_alt = CTRL | ALT;
    let all = SHIFT | CTRL | ALT;
    bindings!(
        ArrowUp,    CTRL; esc("\x1b[1;5A");
        ArrowDown,  CTRL; esc("\x1b[1;5B");
        ArrowLeft,  CTRL; esc("\x1b[1;5D");
        ArrowRight, CTRL; esc("\x1b[1;5C");
        End,        CTRL; esc("\x1b[1;5F");
        Home,       CTRL; esc("\x1b[1;5H");
        Delete,     CTRL; esc("\x1b[3;5~");
        PageUp,     CTRL; esc("\x1b[5;5~");
        PageDown,   CTRL; esc("\x1b[6;5~");
        F1,         CTRL; esc("\x1bO;5P");
        F2,         CTRL; esc("\x1bO;5Q");
        F3,         CTRL; esc("\x1bO;5R");
        F4,         CTRL; esc("\x1bO;5S");
        F5,         CTRL; esc("\x1b[15;5~");
        F6,         CTRL; esc("\x1b[17;5~");
        F7,         CTRL; esc("\x1b[18;5~");
        F8,         CTRL; esc("\x1b[19;5~");
        F9,         CTRL; esc("\x1b[20;5~");
        F10,        CTRL; esc("\x1b[21;5~");
        F11,        CTRL; esc("\x1b[23;5~");
        F12,        CTRL; esc("\x1b[24;5~");
        Enter,      SHIFT; Char('\x0d');
        Backspace,  SHIFT; Char('\x7f');
        Tab,        SHIFT; esc("\x1b[Z");
        End,        SHIFT, +TermMode::ALT_SCREEN; esc("\x1b[1;2F");
        Home,       SHIFT, +TermMode::ALT_SCREEN; esc("\x1b[1;2H");
        PageUp,     SHIFT, +TermMode::ALT_SCREEN; esc("\x1b[5;2~");
        PageDown,   SHIFT, +TermMode::ALT_SCREEN; esc("\x1b[6;2~");
        PageUp,     SHIFT, ~TermMode::ALT_SCREEN; BindingAction::ScrollPageUp;
        PageDown,   SHIFT, ~TermMode::ALT_SCREEN; BindingAction::ScrollPageDown;
        ArrowUp,    SHIFT; esc("\x1b[1;2A");
        ArrowDown,  SHIFT; esc("\x1b[1;2B");
        ArrowLeft,  SHIFT; esc("\x1b[1;2D");
        ArrowRight, SHIFT; esc("\x1b[1;2C");
        Backspace,  ALT; esc("\x1b\x7f");
        End,        ALT; esc("\x1b[1;3F");
        Home,       ALT; esc("\x1b[1;3H");
        Insert,     ALT; esc("\x1b[3;2~");
        Delete,     ALT; esc("\x1b[3;3~");
        PageUp,     ALT; esc("\x1b[5;3~");
        PageDown,   ALT; esc("\x1b[6;3~");
        ArrowUp,    ALT; esc("\x1b[1;3A");
        ArrowDown,  ALT; esc("\x1b[1;3B");
        ArrowLeft,  ALT; esc("\x1b[1;3D");
        ArrowRight, ALT; esc("\x1b[1;3C");
        End,        shift_alt; esc("\x1b[1;4F");
        Home,       shift_alt; esc("\x1b[1;4H");
        ArrowUp,    shift_alt; esc("\x1b[1;4A");
        ArrowDown,  shift_alt; esc("\x1b[1;4B");
        ArrowLeft,  shift_alt; esc("\x1b[1;4D");
        ArrowRight, shift_alt; esc("\x1b[1;4C");
        End,        shift_ctrl; esc("\x1b[1;6F");
        Home,       shift_ctrl; esc("\x1b[1;6H");
        ArrowUp,    shift_ctrl; esc("\x1b[1;6A");
        ArrowDown,  shift_ctrl; esc("\x1b[1;6B");
        ArrowLeft,  shift_ctrl; esc("\x1b[1;6D");
        ArrowRight, shift_ctrl; esc("\x1b[1;6C");
        End,        ctrl_alt; esc("\x1b[1;7F");
        Home,       ctrl_alt; esc("\x1b[1;7H");
        PageUp,     ctrl_alt; esc("\x1b[5;7~");
        PageDown,   ctrl_alt; esc("\x1b[6;7~");
        ArrowUp,    ctrl_alt; esc("\x1b[1;7A");
        ArrowDown,  ctrl_alt; esc("\x1b[1;7B");
        ArrowLeft,  ctrl_alt; esc("\x1b[1;7D");
        ArrowRight, ctrl_alt; esc("\x1b[1;7C");
        End,        all; esc("\x1b[1;8F");
        Home,       all; esc("\x1b[1;8H");
        ArrowUp,    all; esc("\x1b[1;8A");
        ArrowDown,  all; esc("\x1b[1;8B");
        ArrowLeft,  all; esc("\x1b[1;8D");
        ArrowRight, all; esc("\x1b[1;8C");
    )
}

/// Ctrl with a letter, and with Shift too, sends the C0 control character; a few symbols
/// besides.
fn control_characters() -> Vec<(Binding, BindingAction)> {
    let mut rows = Vec::new();
    for (letter, code) in ('a'..='z').zip(0x01u8..) {
        for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::SHIFT] {
            rows.push((
                Binding {
                    target: InputKind::Char(letter.to_string()),
                    modifiers,
                    mode_include: TermMode::empty(),
                    mode_exclude: TermMode::empty(),
                },
                BindingAction::Char(char::from(code)),
            ));
        }
    }
    use BindingAction::Char;
    const CTRL: Modifiers = Modifiers::CTRL;
    let shift_ctrl = Modifiers::SHIFT | CTRL;
    rows.extend(bindings!(
        "[",  CTRL; Char('\x1b');
        "\\", CTRL; Char('\x1c');
        "]",  CTRL; Char('\x1d');
        "-",  CTRL; Char('\x1f');
        "2",  shift_ctrl; Char('\x00');
        "6",  shift_ctrl; Char('\x1e');
        "_",  shift_ctrl; Char('\x1f');
    ));
    rows
}

/// Copy and paste: Ctrl+Shift+C and Ctrl+Shift+V (Cmd+C and Cmd+V on macOS); they replace the
/// control characters of the same chords.
#[cfg(target_os = "macos")]
fn platform_bindings() -> Vec<(Binding, BindingAction)> {
    bindings!(
        "c", Modifiers::LOGO; BindingAction::Copy;
        "v", Modifiers::LOGO; BindingAction::Paste;
        "h", Modifiers::LOGO; BindingAction::Char('\x7f');
    )
}

#[cfg(not(target_os = "macos"))]
fn platform_bindings() -> Vec<(Binding, BindingAction)> {
    let shift_ctrl = Modifiers::SHIFT | Modifiers::CTRL;
    bindings!(
        "c", shift_ctrl; BindingAction::Copy;
        "v", shift_ctrl; BindingAction::Paste;
    )
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
