//! The placeholders in a scene's strings (`scenes.rs`): `{demo}` stands for the demo folder and
//! `{wsl}` for the name of an installed WSL distribution (RC-100-56), so a scene names its files
//! and its distribution wherever they are. Without a distribution - on every platform but
//! Windows, and on Windows without WSL - `{wsl}` stays as written and the scene that names it is
//! skipped, never shot on a made-up name.

use rc_engine::WslDistro;
use serde_json::Value;

/// The placeholder for the demo folder.
pub const DEMO: &str = "{demo}";
/// The placeholder for the WSL distribution.
pub const WSL: &str = "{wsl}";

/// What the placeholders stand for in this run.
#[derive(Debug, Clone, Copy)]
pub struct Places<'a> {
    pub demo: &'a str,
    /// The distribution [`WSL`] names; `None` when there is none.
    pub wsl: Option<&'a str>,
}

impl Places<'_> {
    /// `text` with each placeholder replaced; [`WSL`] stays when there is no distribution.
    #[must_use]
    pub fn fill(&self, text: &str) -> String {
        let text = text.replace(DEMO, self.demo);
        match self.wsl {
            Some(wsl) => text.replace(WSL, wsl),
            None => text,
        }
    }

    /// Every string of `value` filled as [`Places::fill`] does.
    pub fn place(&self, value: &mut Value) {
        match value {
            Value::String(text) if text.contains(DEMO) || text.contains(WSL) => {
                *text = self.fill(text);
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.place(item)),
            Value::Object(fields) => fields.values_mut().for_each(|field| self.place(field)),
            _ => {}
        }
    }
}

/// Whether a string anywhere in `value` - a field's value, not its name - holds `placeholder`.
#[must_use]
pub fn mentions(value: &Value, placeholder: &str) -> bool {
    match value {
        Value::String(text) => text.contains(placeholder),
        Value::Array(items) => items.iter().any(|item| mentions(item, placeholder)),
        Value::Object(fields) => fields.values().any(|field| mentions(field, placeholder)),
        _ => false,
    }
}

/// The distribution [`WSL`] stands for: the default one, else the first listed; `None` when
/// none is installed.
#[must_use]
pub fn pick(distros: &[WslDistro]) -> Option<&str> {
    distros
        .iter()
        .find(|distro| distro.is_default)
        .or_else(|| distros.first())
        .map(|distro| distro.name.as_str())
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
