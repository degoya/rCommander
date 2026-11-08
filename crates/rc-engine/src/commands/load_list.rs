//! `panel.load_list`, Total Commander's `LOADLIST`: the paths of a list file shown in a panel as
//! a flat view (as "Feed to listbox" shows search hits), below the deepest folder that holds them
//! all; a path that is not there is skipped and counted (the flat view's `skipped`). And what
//! the ignore list (RC-100-92) needs of the engine: the list in force for the panels and the
//! queue, and the count of what it hides below the folders a delete is asked for.

use std::{
    path::{Component, PathBuf},
    sync::Arc,
};

use rc_vfs::{EntryKind, VfsPath};

use crate::{
    Engine, EngineError, Outcome, PanelId, TabsError,
    file_filter::{PROGRAMS_KEY, set_programs},
    flat::{FlatPlan, FlatSource},
    ignore_list::{
        IGNORE_ENABLED_KEY, IGNORE_ENTRIES_KEY, IgnoreList, active as active_ignore_list,
        set_active,
    },
    settings::stored_setting,
};

/// The most entries [`Engine::ignored_below`] looks at before it answers with what it counted.
const MAX_IGNORE_WALK: usize = 200_000;

/// The deepest folder that holds every path of `paths` (absolute, at least one): the common part
/// of their folders.
fn common_folder(paths: &[PathBuf]) -> PathBuf {
    let mut common: Vec<Component<'_>> = paths[0]
        .parent()
        .map_or_else(Vec::new, |parent| parent.components().collect());
    for path in &paths[1..] {
        let parent: Vec<Component<'_>> = path
            .parent()
            .map_or_else(Vec::new, |parent| parent.components().collect());
        let same = common
            .iter()
            .zip(&parent)
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(same);
    }
    common.iter().collect()
}

impl Engine {
    /// `panel.load_list`: the absolute paths of the list file `path` in a flat view of the panel.
    pub(crate) async fn load_list(
        &self,
        panel: Option<PanelId>,
        path: &str,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let lines = self.read_list_file(id, path).await?;
        let mut seen = std::collections::HashSet::new();
        let paths: Vec<PathBuf> = lines
            .iter()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.parent().is_some())
            .filter(|path| seen.insert(path.clone()))
            .collect();
        if paths.is_empty() {
            return Err(TabsError::ListFileEmpty.into());
        }
        let root = common_folder(&paths);
        // Paths on another drive than the first one's have no common folder with it: left out.
        let stat: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| path.starts_with(&root) && path.as_path() != root.as_path())
            .collect();
        if root.as_os_str().is_empty() || !root.is_absolute() || stat.is_empty() {
            return Err(TabsError::ListFileEmpty.into());
        }
        let plan = FlatPlan {
            walk: Vec::new(),
            stat,
            groups: Vec::new(),
        };
        self.start_flat(id, VfsPath::local(root), FlatSource::Paths, Arc::new(plan))
    }

    /// The ignore list as the settings say: in force for the panels, the search and the copies
    /// while `ignore_list.enabled` is on; the programs' extensions of the file filters. Read at the
    /// start and on each change of these settings.
    pub(crate) async fn read_panel_list_settings(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        let programs = stored_setting(db, PROGRAMS_KEY).await;
        if let Some(programs) = programs.as_ref().and_then(serde_json::Value::as_str) {
            set_programs(programs);
        }
        let list = if crate::settings::stored_bool(db, IGNORE_ENABLED_KEY).await {
            let entries = stored_setting(db, IGNORE_ENTRIES_KEY).await;
            let text = entries
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            Some(Arc::new(IgnoreList::parse(text)))
        } else {
            None
        };
        self.apply_ignore_list(list);
    }

    /// The ignore list or the programs changed: both read again, and both panels with them.
    pub(crate) async fn panel_lists_changed(&self) {
        self.read_panel_list_settings().await;
        for panel in PanelId::BOTH {
            if let Err(error) = self.refresh(Some(panel)).await {
                tracing::debug!(code = error.code(), "a panel was not read again");
            }
        }
    }

    /// `list` in force from now on - `None` hides nothing - for the panels and the queue.
    pub(crate) fn apply_ignore_list(&self, list: Option<Arc<IgnoreList>>) {
        set_active(list);
        let rule = active_ignore_list().map(|list| list as Arc<dyn rc_ops::IgnoreRule>);
        self.inner.ops.set_ignore(rule);
    }

    /// How many entries the ignore list in force hides below the entries `names` of panel
    /// `panel` - an ignored folder counted once, not entered: what the delete question names, as
    /// a delete takes them along (RC-100-92). 0 while no list is in force. A walk past
    /// [`MAX_IGNORE_WALK`] entries answers with what it counted so far.
    pub async fn ignored_below(&self, panel: PanelId, names: &[String]) -> u64 {
        let Some(list) = active_ignore_list() else {
            return 0;
        };
        let folder = self.state().panel(panel).path.clone();
        let Ok(vfs) = self.vfs(&folder) else {
            return 0;
        };
        let mut pending: Vec<PathBuf> = names.iter().map(|name| folder.path.join(name)).collect();
        let (mut hidden, mut seen) = (0, 0);
        while let Some(dir) = pending.pop() {
            let Ok(entries) = vfs.list(&dir).await else {
                continue;
            };
            let at = VfsPath {
                backend: folder.backend.clone(),
                path: dir.clone(),
            };
            for entry in entries {
                seen += 1;
                if list.hides(&at, &entry) {
                    hidden += 1;
                } else if entry.kind == EntryKind::Dir {
                    pending.push(dir.join(&entry.name));
                }
            }
            if seen >= MAX_IGNORE_WALK {
                break;
            }
        }
        hidden
    }
}

#[cfg(test)]
#[path = "load_list_tests.rs"]
mod tests;
