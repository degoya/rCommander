//! Whether the programs a terminal's shell started end with the terminal and with rCommander
//! (RC-040-06, owner 2026-10-24): on, the default, the whole tree ends - nothing keeps running
//! unseen after rCommander closed; off, only the shell ends and a program started from it (a
//! window it opened) keeps running. Windows only: elsewhere the PTY's end hangs them up. Not
//! protected: it ends no more than the person's own terminal.

use super::{INTEGRATION_KEY, SettingKind, SettingPage, SettingSpec};

pub(super) const INTEGRATION: SettingSpec = SettingSpec {
    key: INTEGRATION_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.shell_integration",
    help: "setting.terminal.shell_integration.help",
    protected: false,
    restart: false,
};

/// Whether what a terminal's shell started ends with it.
pub const END_PROGRAMS_KEY: &str = "terminal.end_programs_on_close";

pub(super) const END_PROGRAMS: SettingSpec = SettingSpec {
    key: END_PROGRAMS_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.end_programs_on_close",
    help: "setting.terminal.end_programs_on_close.help",
    protected: false,
    restart: false,
};

// How a terminal copies its selection (owner 2026-10-25), as Windows Terminal does: Ctrl+C and
// a right click copy while text is selected and otherwise interrupt or paste; copying on
// select is off there too.

/// Ctrl+C copies the selection and clears it; without one it stays the shell's interrupt.
pub const COPY_CTRL_C_KEY: &str = "terminal.copy_ctrl_c";

/// A right click copies the selection and clears it; without one it pastes (Windows).
pub const COPY_RIGHT_CLICK_KEY: &str = "terminal.copy_right_click";

/// Letting go of the mouse after selecting copies the selection.
pub const COPY_ON_SELECT_KEY: &str = "terminal.copy_on_select";

/// Ctrl+V pastes the clipboard's text, as in Windows Terminal; without text, or off, it goes to
/// the program (owner's report 2026-10-25: Claude Code took it for an image paste).
pub const PASTE_CTRL_V_KEY: &str = "terminal.paste_ctrl_v";

/// A program in a terminal may put text on the clipboard (OSC 52), as Claude Code does with what
/// it selected; reading the clipboard that way stays refused (alacritty's `Osc52::OnlyCopy`).
pub const OSC52_COPY_KEY: &str = "terminal.osc52_copy";

/// The clipboard's image pasted into a terminal as a file path (RC-040-08, Ctrl+Alt+V); on a
/// server it is uploaded without asking while this is on (owner 2026-10-25).
pub const PASTE_IMAGE_KEY: &str = "terminal.paste_image";

/// Shift with the mouse selects text even while a program takes the mouse, as in Windows
/// Terminal.
pub const SHIFT_SELECTS_KEY: &str = "terminal.shift_selects";

pub(super) const COPY_CTRL_C: SettingSpec = SettingSpec {
    key: COPY_CTRL_C_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.copy_ctrl_c",
    help: "setting.terminal.copy_ctrl_c.help",
    protected: false,
    restart: false,
};

pub(super) const COPY_RIGHT_CLICK: SettingSpec = SettingSpec {
    key: COPY_RIGHT_CLICK_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.copy_right_click",
    help: "setting.terminal.copy_right_click.help",
    protected: false,
    restart: false,
};

pub(super) const COPY_ON_SELECT: SettingSpec = SettingSpec {
    key: COPY_ON_SELECT_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: false },
    label: "setting.terminal.copy_on_select",
    help: "setting.terminal.copy_on_select.help",
    protected: false,
    restart: false,
};

pub(super) const PASTE_CTRL_V: SettingSpec = SettingSpec {
    key: PASTE_CTRL_V_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.paste_ctrl_v",
    help: "setting.terminal.paste_ctrl_v.help",
    protected: false,
    restart: false,
};

pub(super) const OSC52_COPY: SettingSpec = SettingSpec {
    key: OSC52_COPY_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.osc52_copy",
    help: "setting.terminal.osc52_copy.help",
    protected: false,
    restart: false,
};

pub(super) const SHIFT_SELECTS: SettingSpec = SettingSpec {
    key: SHIFT_SELECTS_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.shift_selects",
    help: "setting.terminal.shift_selects.help",
    protected: false,
    restart: false,
};

pub(super) const PASTE_IMAGE: SettingSpec = SettingSpec {
    key: PASTE_IMAGE_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.paste_image",
    help: "setting.terminal.paste_image.help",
    protected: false,
    restart: false,
};
