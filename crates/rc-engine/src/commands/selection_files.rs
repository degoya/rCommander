//! Total Commander's Mark menu beyond the masks (RC-100-92): "Save selection" into the panel's
//! memory, which Num / ("Restore selection") brings back as it brings back the selection before
//! the last file operation; "Save selection to file", "Load selection from file" and "Load
//! selection from clipboard" (the window reads the clipboard and hands its text; the engine never
//! reads the clipboard). A name the panel does not list is skipped and counted.

use std::{collections::HashMap, io::Write as _, path::Path};

use rc_vfs::{BackendId, VfsError, VfsPath};

use super::list_file::list_lines;
use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, PanelState, TabsError,
    read_guard::from_mcp, recover::LogPanic as _,
};

/// The names of `lines` as `panel` lists them: a name as it is (in a flat view the path below its
/// folder), an absolute path below the panel's folder by its part below it; equal names
/// ignoring case where the exact one is not listed. The names found, and how many were not.
fn listed_names(panel: &PanelState, lines: &[String]) -> (Vec<String>, usize) {
    let exact: HashMap<&str, &str> = panel
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.name.as_str()))
        .collect();
    let folded: HashMap<String, &str> = panel
        .entries
        .iter()
        .map(|entry| (entry.name.to_lowercase(), entry.name.as_str()))
        .collect();
    let (mut found, mut missing) = (Vec::new(), 0);
    for line in lines {
        let name = below(&panel.path, line).unwrap_or_else(|| line.clone());
        let hit = exact
            .get(name.as_str())
            .or_else(|| folded.get(&name.to_lowercase()))
            .copied();
        match hit {
            Some(name) => found.push(name.to_owned()),
            None => missing += 1,
        }
    }
    (found, missing)
}

/// `line` as a path below `folder` (its part below it, the separators the panel shows), when it
/// is an absolute path there.
fn below(folder: &VfsPath, line: &str) -> Option<String> {
    let path = Path::new(line);
    if !path.is_absolute() || folder.backend != BackendId::Local {
        return None;
    }
    let rest = path.strip_prefix(&folder.path).ok()?;
    (!rest.as_os_str().is_empty()).then(|| rest.to_string_lossy().into_owned())
}

/// The text a selection file holds: one name per line, a line break after each.
fn selection_text(panel: &PanelState) -> (String, usize) {
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut text = String::new();
    let mut count = 0;
    // In the panel's order, as Total Commander writes it.
    for entry in panel.entries.iter() {
        if panel.selection.contains(&entry.name) {
            text.push_str(&entry.name);
            text.push_str(newline);
            count += 1;
        }
    }
    (text, count)
}

impl Engine {
    /// `panel.save_selection`: the selection kept in the panel's memory, where Num /
    /// (`panel.restore_selection`) finds it.
    pub(crate) fn save_selection(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            panel.remembered = panel.selection.clone();
            panel.snapshot(id, active)
        };
        Outcome::Panel(snapshot)
    }

    /// `panel.selection_to_file`: the selected names into `path` on this machine, UTF-8. A file
    /// that is there stays unless `overwrite`; over MCP it always stays.
    pub(crate) async fn selection_to_file(
        &self,
        panel: Option<PanelId>,
        path: &str,
        overwrite: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let file = self.local_list_file(id, path)?;
        let (text, names) = selection_text(self.state().panel(id));
        let overwrite = overwrite && !from_mcp();
        let written = {
            let file = file.clone();
            tokio::task::spawn_blocking(move || {
                let mut options = std::fs::OpenOptions::new();
                options.write(true);
                if overwrite {
                    options.create(true).truncate(true);
                } else {
                    options.create_new(true);
                }
                options.open(&file)?.write_all(text.as_bytes())
            })
            .await
            .log_panic("engine.selection_to_file")
        };
        match written {
            Ok(Ok(())) => Ok(Outcome::SelectionWritten {
                path: VfsPath::local(file.clone()),
                names,
            }),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(TabsError::ListFileExists.into())
            }
            Ok(Err(error)) => Err(VfsError::from_io(error, file).into()),
            Err(error) => {
                Err(VfsError::from_io(std::io::Error::other(error.to_string()), file).into())
            }
        }
    }

    /// `panel.selection_from_file`: the entries the file names are selected.
    pub(crate) async fn selection_from_file(
        &self,
        panel: Option<PanelId>,
        path: &str,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let lines = self.read_list_file(id, path).await?;
        Ok(self.select_lines(id, &lines))
    }

    /// `panel.selection_from_text`: the entries `text` names are selected.
    pub(crate) fn selection_from_text(
        &self,
        panel: Option<PanelId>,
        text: &str,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let lines = list_lines(text)?;
        Ok(self.select_lines(id, &lines))
    }

    /// The entries `lines` name selected in panel `id`, besides what is selected already.
    fn select_lines(&self, id: PanelId, lines: &[String]) -> Outcome {
        let (snapshot, selected, missing) = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            let (found, missing) = listed_names(panel, lines);
            let selected = found.len();
            for name in found {
                panel.selection.insert(name);
            }
            (panel.snapshot(id, active), selected, missing)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Selection,
        });
        self.publish(EngineEvent::SelectionLoaded {
            panel: id,
            selected,
            missing,
        });
        Outcome::SelectionLoaded {
            panel: snapshot,
            selected,
            missing,
        }
    }
}

#[cfg(test)]
#[path = "selection_files_tests.rs"]
mod tests;
