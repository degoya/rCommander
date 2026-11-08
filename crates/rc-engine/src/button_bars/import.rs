//! Total Commander's `.bar` files read into a preview (RC-100-87): `[Buttonbar]` with
//! `Buttoncount` and per button `buttonN` (the symbol, `file,index`), `cmdN`, `paramN`, `pathN`,
//! `menuN` (the tooltip) and `iconicN`. An empty `buttonN` and `cmdN` is a separator; a `cmdN`
//! naming another `.bar` is a sub-bar, read too and imported as a bar of its own (at most
//! [`MAX_SUB_BAR_DEPTH`] deep, one that opens a bar above it skipped). Commands are read as the
//! own commands' import reads them (`total_commander_commands::action_of`): `cm_*` through
//! `tc_names`, `em_*` as a reference, a program with its parameters; what rCommander does not
//! take is skipped with its reason.
//!
//! The files are hostile input: each is read with `tc_ini` (a regular file of at most 1 MiB, ANSI
//! or UTF-16 with BOM), only split into text and never run. `%COMMANDER_PATH%` in a symbol or a
//! sub-bar's path is the folder of the file imported, where Total Commander keeps its bars; in a
//! program it stays as written and is rCommander's own folder when the button runs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ButtonBarError, IconRef, MAX_BUTTONS, MAX_SUB_BAR_DEPTH};
use crate::{
    import::{
        tc_ini::{self, Ini},
        total_commander::{TcSkip, menu_name},
        total_commander_commands::{action_of, icon_of},
        total_commander_path::{environment, expand},
    },
    user_commands::{CommandRef, MAX_FIELD},
};

/// The section of a `.bar` file.
const SECTION: &str = "Buttonbar";

/// The most files one import reads: a bar and its sub-bars.
pub const MAX_IMPORT_BARS: usize = rc_db::MAX_BUTTON_BARS;

/// What one button of a `.bar` file becomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PreviewResult {
    /// A button that runs this command; `unknown` the `cm_*` names rCommander does not know,
    /// kept in its rows as written.
    Command {
        action: CommandRef,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        unknown: Vec<String>,
    },
    /// A button that opens the bar at this place of [`ImportPreview::bars`].
    SubBar {
        bar: usize,
    },
    Separator,
    /// Not imported, and why.
    Skipped {
        skip: TcSkip,
    },
}

/// One button of a `.bar` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewItem {
    /// `N` of `buttonN`.
    pub index: usize,
    /// `menuN` without its `&` accelerators.
    #[serde(default)]
    pub tooltip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<IconRef>,
    #[serde(flatten)]
    pub result: PreviewResult,
}

/// One `.bar` file read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewBar {
    /// The file's name, without its folder.
    pub file: String,
    /// The bar's name: the file's name without `.bar`.
    pub name: String,
    pub items: Vec<PreviewItem>,
    /// Buttons past [`MAX_BUTTONS`], left out.
    #[serde(default)]
    pub dropped: usize,
}

/// A `.bar` file and its sub-bars as they would be imported: the file's bar first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub bars: Vec<PreviewBar>,
}

/// How a file is read: from disk, or from a table in the tests.
pub(crate) type Reader<'a> = &'a dyn Fn(&Path) -> Result<Ini, String>;

/// The preview of the `.bar` file at `path` and its sub-bars, read from disk.
///
/// # Errors
///
/// [`ButtonBarError::Import`] when the file itself does not read or has no `[Buttonbar]`; a
/// sub-bar that does not read is a skipped button.
pub fn preview(path: &Path) -> Result<ImportPreview, ButtonBarError> {
    preview_with(path, &|path: &Path| {
        tc_ini::read_file(path).map_err(|error| error.to_string())
    })
}

/// [`preview`] with `read` for the files.
pub(crate) fn preview_with(path: &Path, read: Reader<'_>) -> Result<ImportPreview, ButtonBarError> {
    let ini = read(path).map_err(|reason| ButtonBarError::Import { reason })?;
    if ini.section(SECTION).is_none() {
        return Err(ButtonBarError::Import {
            reason: "the file has no [Buttonbar] section".to_owned(),
        });
    }
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut loader = Loader {
        read,
        folder,
        preview: ImportPreview::default(),
        files: Vec::new(),
    };
    loader.add(path, &ini, &mut Vec::new());
    Ok(loader.preview)
}

/// The files read so far and the bars made of them.
struct Loader<'a> {
    read: Reader<'a>,
    /// The folder of the file imported: `%COMMANDER_PATH%`, and where a relative path starts.
    folder: PathBuf,
    preview: ImportPreview,
    /// The file of each bar of `preview`, compared without regard to case.
    files: Vec<String>,
}

impl Loader<'_> {
    /// Adds the bar of `ini` (read from `path`) and the sub-bars below it; its place in the
    /// preview. `above` holds the files of the bars above it.
    fn add(&mut self, path: &Path, ini: &Ini, above: &mut Vec<String>) -> usize {
        let key = file_key(path);
        let at = self.preview.bars.len();
        self.files.push(key.clone());
        self.preview.bars.push(PreviewBar {
            file: file_name(path),
            name: bar_name(path),
            items: Vec::new(),
            dropped: 0,
        });
        above.push(key);
        let (items, dropped) = self.items(ini, above);
        above.pop();
        let bar = &mut self.preview.bars[at];
        bar.items = items;
        bar.dropped = dropped;
        at
    }

    /// The buttons of `ini`, and how many past [`MAX_BUTTONS`] were left out.
    fn items(&mut self, ini: &Ini, above: &mut Vec<String>) -> (Vec<PreviewItem>, usize) {
        let Some(section) = ini.section(SECTION) else {
            return (Vec::new(), 0);
        };
        let count = section
            .get("Buttoncount")
            .and_then(|count| count.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut items = Vec::new();
        for index in 1..=count.min(MAX_BUTTONS) {
            let value = |key: &str| section.get(&format!("{key}{index}")).unwrap_or_default();
            let cmd = value("cmd").trim();
            let symbol = value("button").trim();
            let tooltip = menu_name(value("menu"));
            let icon = self.icon(symbol);
            let result = if cmd.is_empty() {
                if symbol.is_empty() {
                    PreviewResult::Separator
                } else {
                    PreviewResult::Skipped {
                        skip: TcSkip::NoCommand,
                    }
                }
            } else if !fits(&tooltip) {
                PreviewResult::Skipped {
                    skip: TcSkip::NameInvalid,
                }
            } else if is_bar_file(cmd) {
                self.sub_bar(cmd, above)
            } else {
                match action_of(
                    cmd,
                    value("param"),
                    value("path"),
                    section.get(&format!("iconic{index}")),
                ) {
                    Ok((action, unknown)) => PreviewResult::Command { action, unknown },
                    Err(skip) => PreviewResult::Skipped { skip },
                }
            };
            let tooltip = if fits(&tooltip) {
                tooltip
            } else {
                String::new()
            };
            items.push(PreviewItem {
                index,
                tooltip,
                icon,
                result,
            });
        }
        (items, count.saturating_sub(MAX_BUTTONS))
    }

    /// A sub-bar's file read and added, or why not.
    fn sub_bar(&mut self, cmd: &str, above: &mut Vec<String>) -> PreviewResult {
        let path = self.resolve(unquote(cmd));
        let key = file_key(&path);
        if above.contains(&key) || above.len() > MAX_SUB_BAR_DEPTH {
            return PreviewResult::Skipped {
                skip: TcSkip::TooDeep,
            };
        }
        if let Some(at) = self.files.iter().position(|file| *file == key) {
            return PreviewResult::SubBar { bar: at };
        }
        if self.preview.bars.len() >= MAX_IMPORT_BARS {
            return PreviewResult::Skipped {
                skip: TcSkip::TooMany,
            };
        }
        match (self.read)(&path) {
            Ok(ini) if ini.section(SECTION).is_some() => PreviewResult::SubBar {
                bar: self.add(&path, &ini, above),
            },
            _ => PreviewResult::Skipped {
                skip: TcSkip::InvalidPath,
            },
        }
    }

    /// The symbol `buttonN` names, `%COMMANDER_PATH%` the imported file's folder; `None` for none
    /// or one that does not fit.
    fn icon(&self, symbol: &str) -> Option<IconRef> {
        if symbol.is_empty() {
            return None;
        }
        let (file, index) = icon_of(Some(symbol));
        let file = self.expanded(&file);
        (!file.is_empty() && fits(&file)).then_some(IconRef { path: file, index })
    }

    /// `text` with the known `%VAR%` filled in, `%COMMANDER_PATH%` the imported file's folder.
    fn expanded(&self, text: &str) -> String {
        let folder = self.folder.to_string_lossy().into_owned();
        expand(text, &|name: &str| environment(name, Some(&folder)))
    }

    /// A sub-bar's path: variables filled in, a relative one in the imported file's folder.
    fn resolve(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(self.expanded(path));
        if path.is_absolute() {
            path
        } else {
            self.folder.join(path)
        }
    }
}

/// Whether `cmd` names a `.bar` file: a sub-bar.
fn is_bar_file(cmd: &str) -> bool {
    unquote(cmd).to_ascii_lowercase().ends_with(".bar")
}

/// `raw` without the quotes around it.
fn unquote(raw: &str) -> &str {
    let raw = raw.trim();
    raw.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map_or(raw, str::trim)
}

/// Whether `text` fits a field: no control character, at most [`MAX_FIELD`] characters.
fn fits(text: &str) -> bool {
    !text.chars().any(char::is_control) && text.chars().count() <= MAX_FIELD
}

/// `path` as the files are compared: without regard to case, as Windows names them.
fn file_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// The file's name without its folder.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A bar's name from its file: the name without `.bar`, at most the longest name a bar has.
fn bar_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().trim().to_owned())
        .filter(|stem| !stem.is_empty() && !stem.chars().any(char::is_control))
        .unwrap_or_else(|| "Imported".to_owned());
    stem.chars().take(rc_db::MAX_BUTTON_BAR_NAME).collect()
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
