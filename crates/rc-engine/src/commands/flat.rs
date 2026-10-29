//! `panel.branch_view` (Ctrl+B, Ctrl+Shift+B), `panel.feed_paths` ("Feed to listbox") and
//! `files.find_dialog` (Alt+F7): the flat view of a panel and the way into the find-files dialog.
//!
//! A flat view starts empty and is filled by a task of its own: the paths of its plan are read
//! one by one, its folders walked with the file search's walk (files only, links not followed,
//! hidden entries as the panels show them). What was found is shown sorted every
//! [`FLUSH_EVERY`] or so - less often as the list grows, so the sorting stays a small part of the
//! work - and once more at the end. Each flush checks that the panel still shows this run; when
//! it does not (the panel went elsewhere, or a refresh started a new run) the task ends there.

use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use rc_vfs::{Entry, Vfs, VfsError, VfsPath};

use super::panel::resolve_path;
use crate::{
    CursorTarget, Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, PanelState,
    flat::{FlatPlan, FlatSource, FlatView, MAX_FLAT_ENTRIES, flat_name, next_run},
    search::{Filters, KindFilter, SearchCancel, SearchEvent, SearchQuery, Walk},
    sort::{names_for, sort_entries},
    state::{by_name, is_hidden},
};

/// How often a growing flat view (and a listing still arriving, `panel_listing.rs`) is shown at
/// least.
pub(super) const FLUSH_EVERY: Duration = Duration::from_millis(200);
/// Entries per millisecond between two flushes of a long list: 100 000 entries are shown about
/// every half second.
pub(super) const ENTRIES_PER_MS: usize = 200;

impl Engine {
    /// `panel.branch_view`: `show` starts the flat view of the panel's folder (with `selection`,
    /// of the selected folders and files, or the entry under the cursor); not `show` leaves it
    /// into the folder of the entry under the cursor, with the cursor on it; `None` switches.
    pub(crate) async fn branch_view(
        &self,
        panel: Option<PanelId>,
        selection: bool,
        show: Option<bool>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let flat = self.state().panel(id).flat.is_some();
        if !show.unwrap_or(!flat) {
            return if flat {
                self.leave_flat(id).await
            } else {
                Ok(Outcome::Panel(self.snapshot(Some(id))))
            };
        }
        let (path, source, plan) = {
            let state = self.state();
            let panel = state.panel(id);
            if selection {
                (panel.path.clone(), FlatSource::Selection, selected(panel))
            } else {
                let plan = FlatPlan {
                    walk: vec![panel.path.path.clone()],
                    stat: Vec::new(),
                };
                (panel.path.clone(), FlatSource::Branch, plan)
            }
        };
        self.start_flat(id, path, source, Arc::new(plan))
    }

    /// `panel.feed_paths`: the panel shows `root` (its own folder when `None`) flat with the
    /// entries at `paths` (relative ones from `root`), each once; missing ones are counted as
    /// skipped.
    pub(crate) async fn feed_paths(
        &self,
        panel: Option<PanelId>,
        root: Option<&str>,
        paths: &[String],
    ) -> Result<Outcome, EngineError> {
        if paths.len() > MAX_FLAT_ENTRIES {
            return Err(EngineError::InvalidSearch {
                reason: format!("at most {MAX_FLAT_ENTRIES} paths"),
            });
        }
        let id = self.resolve(panel);
        let current = self.state().panel(id).path.clone();
        let root = root.map_or_else(|| current.clone(), |root| resolve_path(&current, root));
        let vfs = self.vfs(&root)?;
        if !vfs.stat(&root.path).await?.is_dir_like() {
            return Err(VfsError::NotADirectory { path: root.path }.into());
        }
        let mut seen = HashSet::new();
        let stat = paths
            .iter()
            .map(|path| resolve_path(&root, path).path)
            .filter(|path| seen.insert(path.clone()))
            .collect();
        let plan = FlatPlan {
            walk: Vec::new(),
            stat,
        };
        self.start_flat(id, root, FlatSource::Paths, Arc::new(plan))
    }

    /// `files.find_dialog`: announces that the find-files dialog is wanted for the panel.
    pub(crate) fn find_dialog(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        self.publish(EngineEvent::FindFiles { panel: id });
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// `panel.refresh` of a flat view: the same view read again. `None` when the panel shows no
    /// flat view.
    pub(crate) fn refresh_flat(&self, id: PanelId) -> Option<Result<Outcome, EngineError>> {
        let (path, flat) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.flat.clone()?)
        };
        Some(self.start_flat(id, path, flat.source, flat.plan))
    }

    /// The folder a flat view was made from, for the Ctrl+R command; `None` when the panel shows
    /// no flat view.
    pub(crate) fn flat_root(&self, id: PanelId) -> Option<VfsPath> {
        let state = self.state();
        let panel = state.panel(id);
        panel.flat.as_ref().map(|_| panel.path.clone())
    }

    /// The panel leaves its flat view into the folder of the entry under the cursor, the cursor
    /// on it; an empty view returns to its own folder.
    async fn leave_flat(&self, id: PanelId) -> Result<Outcome, EngineError> {
        let (root, name) = {
            let state = self.state();
            let panel = state.panel(id);
            let name = panel.cursor_entry().map(|entry| entry.name.clone());
            (panel.path.clone(), name)
        };
        let target = name.map(|name| root.join(name));
        let (Some(dir), Some(file)) = (
            target.as_ref().and_then(VfsPath::parent),
            target
                .as_ref()
                .and_then(|target| target.path.file_name())
                .map(|file| file.to_string_lossy().into_owned()),
        ) else {
            return self.show_path(id, root).await;
        };
        self.show_path(id, dir).await?;
        match self.set_cursor(Some(id), CursorTarget::Name(file)) {
            Ok(outcome) => Ok(outcome),
            // Gone, or hidden, meanwhile: the folder is shown all the same.
            Err(_) => Ok(Outcome::Panel(self.snapshot(Some(id)))),
        }
    }

    /// The panel shows `path` flat and empty, and a task fills it from `plan`.
    fn start_flat(
        &self,
        id: PanelId,
        path: VfsPath,
        source: FlatSource,
        plan: Arc<FlatPlan>,
    ) -> Result<Outcome, EngineError> {
        let vfs = self.vfs(&path)?;
        let run = next_run();
        let (snapshot, show_hidden) = {
            let mut state = self.state();
            let show_hidden = state.show_hidden;
            let active = state.active == id;
            let panel = state.panel_mut(id);
            let mut next = panel.next(path.clone(), Vec::new(), panel.sort, show_hidden);
            next.flat = Some(FlatView {
                source,
                loading: true,
                truncated: false,
                skipped: 0,
                plan: Arc::clone(&plan),
                run,
            });
            *panel = next;
            (panel.snapshot(id, active), show_hidden)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Navigated,
        });
        let filler = Filler {
            engine: self.clone(),
            panel: id,
            run,
            found: Vec::new(),
            skipped: 0,
            truncated: false,
            last_flush: Instant::now(),
        };
        tokio::spawn(filler.fill(vfs, path, plan, show_hidden));
        Ok(Outcome::Panel(snapshot))
    }
}

/// The plan of Ctrl+Shift+B: the selected folders walked, the selected files as they are; the
/// entry under the cursor when nothing is selected, the whole folder in an empty one.
fn selected(panel: &PanelState) -> FlatPlan {
    let names: Vec<&String> = if panel.selection.is_empty() {
        panel
            .cursor_entry()
            .map(|entry| &entry.name)
            .into_iter()
            .collect()
    } else {
        panel.selection.iter().collect()
    };
    let mut plan = FlatPlan::default();
    // One index for every selected name: the folder is not searched once per name (RC-080-45).
    let listed = by_name(&panel.entries);
    for name in names {
        let Some(entry) = listed
            .get(name.as_str())
            .and_then(|&at| panel.entries.get(at))
        else {
            continue;
        };
        let path = panel.path.path.join(name);
        if entry.is_dir_like() {
            plan.walk.push(path);
        } else {
            plan.stat.push(path);
        }
    }
    if plan.walk.is_empty() && plan.stat.is_empty() {
        plan.walk.push(panel.path.path.clone());
    }
    plan
}

/// The task that fills one run of a flat view.
struct Filler {
    engine: Engine,
    panel: PanelId,
    run: u64,
    found: Vec<Entry>,
    skipped: u64,
    truncated: bool,
    last_flush: Instant,
}

impl Filler {
    async fn fill(mut self, vfs: Arc<dyn Vfs>, root: VfsPath, plan: Arc<FlatPlan>, shown: bool) {
        for path in &plan.stat {
            match vfs.stat(path).await {
                Ok(mut entry) if shown || !is_hidden(&entry) => {
                    entry.name = flat_name(&root.path, path);
                    self.push(entry);
                }
                Ok(_) => {}
                Err(_) => self.skipped += 1,
            }
            if self.truncated {
                break;
            }
            if !self.tick() {
                return;
            }
        }
        if !plan.walk.is_empty() && !self.truncated && !self.walk(vfs, &root, &plan, shown).await {
            return;
        }
        self.flush(true);
    }

    /// The files below the plan's folders; `false` once the panel shows something else.
    async fn walk(
        &mut self,
        vfs: Arc<dyn Vfs>,
        root: &VfsPath,
        plan: &FlatPlan,
        shown: bool,
    ) -> bool {
        let query = SearchQuery {
            filters: Filters {
                kind: KindFilter::Files,
                ..Filters::default()
            },
            skip_hidden: !shown,
            ..SearchQuery::default()
        };
        // No pattern and no text: nothing that could fail to compile.
        let Ok(compiled) = query.compile() else {
            return true;
        };
        let mut walk = Walk::new(
            vfs,
            root.backend.clone(),
            plan.walk.clone(),
            compiled,
            SearchCancel::default(),
        );
        while let Some(event) = walk.next().await {
            match event {
                SearchEvent::Hit(hit) if shown || !is_hidden(&hit.entry) => {
                    let mut entry = hit.entry;
                    entry.name = flat_name(&root.path, &hit.path.path);
                    self.push(entry);
                    if self.truncated {
                        return true;
                    }
                }
                SearchEvent::Skipped { .. } => self.skipped += 1,
                SearchEvent::Hit(_)
                | SearchEvent::Progress { .. }
                | SearchEvent::Finished { .. } => {}
            }
            if !self.tick() {
                return false;
            }
        }
        true
    }

    fn push(&mut self, entry: Entry) {
        self.found.push(entry);
        self.truncated = self.found.len() >= MAX_FLAT_ENTRIES;
    }

    /// Shows what was found when it is time; `false` once the panel shows something else.
    fn tick(&mut self) -> bool {
        let every = FLUSH_EVERY.max(Duration::from_millis(
            (self.found.len() / ENTRIES_PER_MS) as u64,
        ));
        self.last_flush.elapsed() < every || self.flush(false)
    }

    /// The entries found so far, sorted in the panel's order, into the panel - with `done`, as
    /// the finished view. `false` when the panel no longer shows this run. The next flush is due
    /// counted from the end of this one: a sort slower than the interval (a long view in a debug
    /// build) would otherwise run the flushes back to back, one entry apart.
    fn flush(&mut self, done: bool) -> bool {
        loop {
            let (order, names) = {
                let state = self.engine.state();
                let panel = state.panel(self.panel);
                if !self.shown_in(panel) {
                    return false;
                }
                (panel.sort, names_for(&panel.path))
            };
            // Sorted outside the lock: the state never waits on a long sort.
            let mut sorted = self.found.clone();
            sort_entries(&mut sorted, order, names);
            {
                let mut state = self.engine.state();
                let panel = state.panel_mut(self.panel);
                if !self.shown_in(panel) {
                    return false;
                }
                if panel.sort != order {
                    // Sorted another way meanwhile: the next flush has it, the last one waits.
                    if done {
                        continue;
                    }
                    self.last_flush = Instant::now();
                    return true;
                }
                let cursor = panel.cursor_entry().map(|entry| entry.name.clone());
                panel.entries = sorted.into();
                panel.cursor = cursor.and_then(|name| panel.index_of(&name)).unwrap_or(0);
                if let Some(flat) = panel.flat.as_mut() {
                    flat.loading = !done;
                    flat.truncated = self.truncated;
                    flat.skipped = self.skipped;
                }
            }
            self.engine.publish(EngineEvent::Panel {
                panel: self.panel,
                change: PanelChange::Listing,
            });
            self.last_flush = Instant::now();
            return true;
        }
    }

    fn shown_in(&self, panel: &PanelState) -> bool {
        panel.flat.as_ref().is_some_and(|flat| flat.run == self.run)
    }
}

#[cfg(test)]
#[path = "flat_tests.rs"]
mod tests;
