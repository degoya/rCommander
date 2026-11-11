//! The button bars (RC-100-87), as Total Commander has them: a bar of buttons above the panels
//! and a second, vertical one between them, each showing one of the person's bars (`buttonbar.*`
//! settings). A bar is kept in `rc_db`'s `button_bars` as versioned JSON ([`BarSpec`],
//! `spec.rs`): buttons that run a command of the one command model ([`crate::CommandRef`],
//! RC-100-86) or open another bar as a sub-bar, with a symbol and a tooltip, and separators.
//!
//! `store.rs` lists, saves, renames and deletes bars - a sub-bar must exist, no bar opens one
//! above it and none nests deeper than [`MAX_SUB_BAR_DEPTH`] (`spec::check_links`); `run.rs` runs
//! a button for a panel through the own commands' run, with files dropped on it as further
//! arguments; `import.rs` reads Total Commander's `.bar` files into a preview and imports them
//! with their sub-bars. The first start's bar "Default" is migration 0033's
//! (`default_bar.rs`).

mod default_bar;
mod error;
mod import;
mod run;
mod spec;
mod store;

use serde::{Deserialize, Serialize};

pub use default_bar::{DEFAULT_BAR_NAME, DEFAULT_ROWS, default_spec};
pub use error::ButtonBarError;
pub use import::{ImportPreview, MAX_IMPORT_BARS, PreviewBar, PreviewItem, PreviewResult, preview};
pub use spec::{
    BarButton, BarItem, BarSpec, ButtonAction, IconRef, MAX_BUTTONS, MAX_SUB_BAR_DEPTH,
    SPEC_VERSION, check_links, clean_name,
};

use crate::{
    Engine, EngineEvent, Outcome,
    settings::{
        BUTTON_BAR_KEY, COMMAND_LINE_KEY, DRIVE_BAR_KEY, FKEY_BAR_KEY, STATUS_BAR_KEY,
        VERTICAL_BUTTON_BAR_KEY,
    },
};

/// The id of a stored button bar.
pub type ButtonBarId = i64;

/// The most button bars kept.
pub const MAX_BUTTON_BARS: usize = rc_db::MAX_BUTTON_BARS;

/// Where a button bar stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarPlace {
    /// Above the panels (Alt+Shift+F11).
    #[default]
    Horizontal,
    /// Between the panels (Alt+Shift+F12).
    Vertical,
}

/// The start of a bar's row name, [`BarKind::toggle_row`].
const TOGGLE_ROW_PREFIX: &str = "toggle_bar.";

/// A bar of the window the View menu shows or hides (`view.toggle_bar`), each a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarKind {
    ButtonBar,
    VerticalButtonBar,
    DriveBar,
    CommandLine,
    StatusBar,
    FunctionKeyBar,
}

impl BarKind {
    /// Every bar, in the View menu's order.
    pub const ALL: [Self; 6] = [
        Self::ButtonBar,
        Self::VerticalButtonBar,
        Self::DriveBar,
        Self::CommandLine,
        Self::StatusBar,
        Self::FunctionKeyBar,
    ];

    /// The setting that shows it.
    #[must_use]
    pub fn setting_key(self) -> &'static str {
        match self {
            Self::ButtonBar => BUTTON_BAR_KEY,
            Self::VerticalButtonBar => VERTICAL_BUTTON_BAR_KEY,
            Self::DriveBar => DRIVE_BAR_KEY,
            Self::CommandLine => COMMAND_LINE_KEY,
            Self::StatusBar => STATUS_BAR_KEY,
            Self::FunctionKeyBar => FKEY_BAR_KEY,
        }
    }

    /// The name of its serde form: `button_bar`, `drive_bar`, ...
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ButtonBar => "button_bar",
            Self::VerticalButtonBar => "vertical_button_bar",
            Self::DriveBar => "drive_bar",
            Self::CommandLine => "command_line",
            Self::StatusBar => "status_bar",
            Self::FunctionKeyBar => "function_key_bar",
        }
    }

    /// The command that shows or hides it: Total Commander's `cm_VisButtonBar` and
    /// `cm_VisButtonBar2` for the button bars (RC-110-15), `view.toggle_bar` for the others.
    #[must_use]
    pub fn toggle_command(self) -> crate::Command {
        match self {
            Self::ButtonBar => crate::Command::ToggleButtonBar {},
            Self::VerticalButtonBar => crate::Command::ToggleVerticalButtonBar {},
            bar => crate::Command::ToggleBar { bar },
        }
    }

    /// The name of the row that shows or hides it, the command palette's and the View menu's:
    /// `toggle_bar.<bar>`. No key runs it (Total Commander has none); a Total Commander name
    /// maps to it (`user_commands::tc_names`).
    #[must_use]
    pub fn toggle_row(self) -> String {
        format!("{TOGGLE_ROW_PREFIX}{}", self.as_str())
    }

    /// The bar whose row [`Self::toggle_row`] is `name`.
    #[must_use]
    pub fn of_toggle_row(name: &str) -> Option<Self> {
        let bar = name.strip_prefix(TOGGLE_ROW_PREFIX)?;
        Self::ALL.into_iter().find(|kind| kind.as_str() == bar)
    }
}

/// One button bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonBar {
    pub id: ButtonBarId,
    pub name: String,
    pub spec: BarSpec,
}

/// What the `button_bar.*` commands answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum ButtonBarsOutcome {
    /// `button_bar.list`: every bar with its buttons, by id.
    Bars { bars: Vec<ButtonBar> },
    /// `button_bar.get`.
    Bar { bar: ButtonBar },
    /// `button_bar.save`, `button_bar.rename`: the bar saved.
    Saved { id: ButtonBarId },
    /// `button_bar.import_preview`: what an import would make.
    Preview { preview: ImportPreview },
    /// `button_bar.import`: the bars made, the file's bar first.
    Imported { bars: Vec<ButtonBarId> },
    /// `button_bar.run`: the programs started; `asking` when the window asks for a program's
    /// parameters first (`?`), which then runs the button again with them.
    Ran { started: usize, asking: bool },
    /// `button_bar.delete`.
    Done {},
}

/// A `button_bar.*` answer.
pub(crate) fn answer(outcome: ButtonBarsOutcome) -> Outcome {
    Outcome::ButtonBars(Box::new(outcome))
}

impl Engine {
    /// `button_bar.manager`: the window opens the button bars' window.
    pub(crate) fn button_bar_window(&self) -> Outcome {
        self.publish(EngineEvent::ButtonBarWindow);
        Outcome::Panel(self.snapshot(None))
    }

    /// `button_bar.focus`: the window gives the keyboard to the bar at `bar`, when it shows.
    pub(crate) fn focus_button_bar(&self, bar: BarPlace) -> Outcome {
        self.publish(EngineEvent::ButtonBarFocus { bar });
        Outcome::Panel(self.snapshot(None))
    }
}

/// One command of each variant, for the registry's tests.
#[cfg(test)]
pub(crate) fn one_of_each() -> Vec<crate::Command> {
    use crate::{Command, PanelId};
    vec![
        Command::ListButtonBars {},
        Command::GetButtonBar { id: 1 },
        Command::SaveButtonBar {
            id: None,
            name: "Tools".into(),
            spec: default_spec(),
        },
        Command::RenameButtonBar {
            id: 1,
            name: "Main".into(),
        },
        Command::DeleteButtonBar { id: 2 },
        Command::PreviewButtonBarImport {
            path: "C:\\totalcmd\\default.bar".into(),
        },
        Command::ImportButtonBar {
            path: "C:\\totalcmd\\default.bar".into(),
        },
        Command::RunButton {
            bar: 1,
            button: 0,
            panel: Some(PanelId::Left),
            params: None,
            files: vec!["C:\\readme.txt".into()],
        },
        Command::ButtonBarManager {},
        Command::FocusButtonBar {
            bar: BarPlace::Vertical,
        },
        Command::ToggleBar {
            bar: BarKind::StatusBar,
        },
        Command::ToggleButtonBar {},
        Command::ToggleVerticalButtonBar {},
    ]
}

#[cfg(test)]
#[path = "button_bars_tests.rs"]
mod tests;
