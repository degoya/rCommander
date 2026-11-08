//! "Go to file" and "Feed to listbox": a hit shown in the panel, or every hit flat in it.
//!
//! A hit inside an archive (RC-100-93) is gone to through the archive file
//! (`C:\data\a.zip\docs`), which opens the archive in the panel with the cursor on the entry, so F3
//! and F4 then work on it as in any archive. "Feed to listbox" follows Total Commander: "If all
//! files were found in a single archive, the files will be shown in the source file window.
//! Otherwise the archives themselves will be shown" - each once, beside the hits outside any
//! archive.

use std::collections::HashSet;

use rc_engine::{Command, CursorTarget, PanelId, SearchHit, archive_file, shown_path};

use crate::app::{App, Effect};

impl App {
    /// "Go to file": the panel shows the hit's folder, the cursor on the hit; the dialog closes.
    pub(super) fn go_to_hit(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.search.as_ref() else {
            return Vec::new();
        };
        let panel = dialog.panel;
        let Some(hit) = dialog.results.current() else {
            return Vec::new();
        };
        let (Some(folder), Some(name)) = (hit.path.parent(), hit.path.path.file_name()) else {
            return Vec::new();
        };
        let mut commands = self.activating(panel);
        commands.push(Command::Navigate {
            panel: Some(panel),
            path: shown_path(&folder).display().to_string(),
        });
        commands.push(Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name.to_string_lossy().into_owned()),
        });
        self.close_search();
        vec![Effect::Dispatch(panel, commands)]
    }

    /// "Feed to listbox": the hits flat in the panel ([`fed`]).
    pub(super) fn feed_hits(&mut self) -> Vec<Effect> {
        let Some(dialog) = self
            .search
            .as_ref()
            .filter(|dialog| !dialog.results.is_empty())
        else {
            return Vec::new();
        };
        let panel = dialog.panel;
        // A duplicate result keeps each file's group (RC-100-98); it never lies in an archive.
        let (root, paths, groups) = if dialog.results.is_duplicates() {
            let paths = dialog
                .results
                .hits()
                .iter()
                .map(|hit| hit.path.path.display().to_string())
                .collect();
            let groups = dialog.results.group_numbers().to_vec();
            (dialog.root.clone(), paths, groups)
        } else {
            let (root, paths) = fed(dialog.results.hits(), dialog.root.clone());
            (root, paths, Vec::new())
        };
        let mut commands = self.activating(panel);
        commands.push(Command::FeedPaths {
            panel: Some(panel),
            root,
            paths,
            groups,
        });
        self.close_search();
        vec![Effect::Dispatch(panel, commands)]
    }

    /// The command that makes `panel` the active one, unless it is.
    fn activating(&self, panel: PanelId) -> Vec<Command> {
        if panel == self.active {
            Vec::new()
        } else {
            vec![Command::SwitchActivePanel { panel: Some(panel) }]
        }
    }
}

/// The folder the panel shows and the paths fed into it: every hit in the first start folder
/// `root`; the hits of one archive in that archive, when all lie in it; else each archive with
/// hits once, in place of its hits.
fn fed(hits: &[SearchHit], root: Option<String>) -> (Option<String>, Vec<String>) {
    let mut archives = hits.iter().map(|hit| archive_file(&hit.path));
    if let Some(Some(only)) = archives.next()
        && archives.all(|archive| archive == Some(only))
    {
        let entries = hits
            .iter()
            .map(|hit| {
                hit.path
                    .path
                    .to_string_lossy()
                    .trim_start_matches(['/', '\\'])
                    .to_owned()
            })
            .collect();
        return (Some(shown_path(only).display().to_string()), entries);
    }
    let mut seen = HashSet::new();
    let paths = hits
        .iter()
        .filter_map(|hit| match archive_file(&hit.path) {
            Some(archive) => seen
                .insert(&archive.path)
                .then(|| shown_path(archive).display().to_string()),
            None => Some(hit.path.path.display().to_string()),
        })
        .collect();
    (root, paths)
}
