//! The disk-usage window (Ctrl+Shift+L, RC-100-98, rCommander's own): the active panel's folder by
//! size, as ncdu and TreeSize show it. The engine walks the folder in the background
//! ([`Engine::start_disk_usage`]); the window reads the tree every 200 ms while the walk goes on,
//! so the list fills as it runs, and Escape stops it with what was found. The tab "Folders" lists
//! the subfolders of the folder shown - and its own files on one line - largest first, with their
//! share, files and folders: Enter goes into one, Backspace up, Ctrl+Enter shows it in the active
//! panel with the cursor on it, Ctrl+R reads everything again. The tab "Largest files" lists the
//! largest files, which "Feed to listbox" shows flat in the panel. Nothing is deleted here: F8
//! in the panel asks as always. `disk_usage.size` chooses bytes or occupied bytes, and
//! `disk_usage.largest_files` how many files the second tab keeps.

use iced::{
    Task,
    futures::{StreamExt as _, stream},
};
use rc_engine::{
    Chord, Command, CursorTarget, DirUsage, Engine, PanelId, VfsPath,
    disk_usage::{DiskUsageHandle, FolderRow, LargeFile, REFRESH_EVERY, UsageChoices, UsageTree},
};

use super::ToolsMsg;
use crate::app::{
    App, Effect, Message, dialog_windows::DialogWindow, field_walk, list_window::ListWindow,
};
use crate::i18n::error_text;

/// The rows a list shows until it knows its height.
const ROWS: usize = 12;

/// The window's two tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageTab {
    Folders,
    Largest,
}

/// How the walk stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stand {
    /// Asked of the engine, not started yet.
    Starting,
    Running,
    Done,
    Stopped,
    Failed(String),
}

/// The open window.
#[derive(Debug)]
pub struct DiskUsageWindow {
    /// The panel it was opened for and shows folders in.
    pub panel: PanelId,
    /// The folder analysed.
    pub root: VfsPath,
    /// Numbers the walks over every window of the session (`ToolsUi::next_usage`), so a late
    /// start or tick of an earlier one - an earlier window's too - is not taken for this one's.
    generation: u64,
    handle: Option<DiskUsageHandle>,
    pub choices: UsageChoices,
    pub tab: UsageTab,
    /// The node of the folder shown.
    pub folder: usize,
    /// Where the folder shown is, relative to the root.
    pub relative: String,
    /// Everything below the folder shown.
    pub total: DirUsage,
    pub rows: Vec<FolderRow>,
    pub largest: Vec<LargeFile>,
    pub list: ListWindow,
    pub files: ListWindow,
    pub stand: Stand,
    pub truncated: bool,
    /// The folder and the sum `rows` were built from, and the root's sum `largest` was: a tick
    /// that finds them unchanged builds nothing.
    seen_rows: Option<(usize, DirUsage)>,
    seen_largest: Option<DirUsage>,
}

impl DiskUsageWindow {
    fn new(panel: PanelId, root: VfsPath, generation: u64) -> Self {
        Self {
            panel,
            root,
            generation,
            handle: None,
            choices: UsageChoices::default(),
            tab: UsageTab::Folders,
            folder: UsageTree::ROOT,
            relative: String::new(),
            total: DirUsage::default(),
            rows: Vec::new(),
            largest: Vec::new(),
            list: ListWindow::new(ROWS),
            files: ListWindow::new(ROWS),
            stand: Stand::Starting,
            truncated: false,
            seen_rows: None,
            seen_largest: None,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.stand, Stand::Starting | Stand::Running)
    }

    /// The row under the cursor of the folder list.
    pub fn current(&self) -> Option<&FolderRow> {
        self.rows.get(self.list.cursor())
    }

    /// The tree read again: the sums of the folder shown and the list of the tab shown - and that
    /// only when its sum moved since it was built, since every entry the walk adds changes the
    /// sum of each folder above it. The other tab's list is read when it is switched to.
    fn refresh(&mut self) {
        let Some(handle) = &self.handle else {
            return;
        };
        let (by, folder, tab) = (self.choices.size_by, self.folder, self.tab);
        let (seen_rows, seen_largest) = (self.seen_rows, self.seen_largest);
        let read = handle.read(|tree| {
            let usage = |node| tree.node(node).map(|node| node.usage).unwrap_or_default();
            let (total, all) = (usage(folder), usage(UsageTree::ROOT));
            let rows = (tab == UsageTab::Folders && seen_rows != Some((folder, total)))
                .then(|| tree.rows(folder, by));
            let largest =
                (tab == UsageTab::Largest && seen_largest != Some(all)).then(|| tree.largest());
            let relative = tree.relative(folder).display().to_string();
            let ended = (tree.finished(), tree.cancelled(), tree.failed());
            (rows, largest, total, all, relative, ended, tree.truncated())
        });
        let (rows, largest, total, all, relative, (finished, cancelled, failed), truncated) = read;
        self.total = total;
        self.relative = relative;
        self.truncated = truncated;
        self.stand = match (finished, failed) {
            (_, Some(code)) => Stand::Failed(error_text(code)),
            (false, None) => Stand::Running,
            (true, None) if cancelled => Stand::Stopped,
            (true, None) => Stand::Done,
        };
        if let Some(rows) = rows {
            let cursor = self.current().map(|row| (row.node, row.name.clone()));
            self.rows = rows;
            self.seen_rows = Some((folder, total));
            // The cursor stays on its folder while the rows move with the sums.
            let at = cursor.and_then(|(node, name)| {
                self.rows
                    .iter()
                    .position(|row| row.node == node && row.name == name)
            });
            if let Some(at) = at {
                self.list.set_cursor(at);
            }
            self.list.fit(self.list.page(), self.rows.len());
        }
        if let Some(largest) = largest {
            self.largest = largest;
            self.seen_largest = Some(all);
            self.files.fit(self.files.page(), self.largest.len());
        }
    }

    /// The folder `node` is shown, the cursor on its first row - or on `back_to`, the folder it
    /// came up from.
    fn show(&mut self, node: usize, back_to: Option<usize>) {
        self.folder = node;
        self.list.reset();
        self.refresh();
        if let Some(at) =
            back_to.and_then(|child| self.rows.iter().position(|row| row.node == Some(child)))
        {
            self.list.point_at(at, self.rows.len());
        }
    }
}

/// What the window's controls and its walk say.
#[derive(Debug, Clone)]
pub enum UsageMsg {
    /// The walk of the window numbered so started - with the settings read - or the error code
    /// why not.
    Started(u64, Result<(DiskUsageHandle, UsageChoices), &'static str>),
    /// Time to read the tree of the walk numbered so again.
    Tick(u64),
    Tab(UsageTab),
    /// A folder row clicked, by its index; double-clicked: into it.
    Click(usize),
    Open(usize),
    Scroll(isize),
    Viewport(usize),
    /// A row of the largest files clicked, the wheel over them, their list's height.
    FileClick(usize),
    FileScroll(isize),
    FileViewport(usize),
    Up,
    ShowInPanel,
    Reread,
    Stop,
    Close,
    Feed,
}

pub(super) fn wrap(message: UsageMsg) -> Message {
    Message::Tools(ToolsMsg::Usage(message))
}

impl App {
    /// Ctrl+Shift+L: the window opens for `panel`'s folder and its walk starts; an open window
    /// comes to the front.
    pub(in crate::app) fn open_disk_usage(&mut self, panel: PanelId) -> Vec<Effect> {
        if self.tools.usage.is_some() {
            return self.dialog_window_front(DialogWindow::DiskUsage);
        }
        let root = self.pane(panel).state.path.clone();
        let generation = self.tools.next_usage();
        self.tools.usage = Some(DiskUsageWindow::new(panel, root, generation));
        vec![Effect::Task(start(self.engine.clone(), panel, generation))]
    }

    pub(super) fn on_disk_usage(&mut self, message: UsageMsg) -> Vec<Effect> {
        // A walk whose window closed or read again before it started has no one to stop it
        // later: it stops here.
        if let UsageMsg::Started(generation, started) = &message
            && self.tools.usage.as_ref().map(|window| window.generation) != Some(*generation)
        {
            if let Ok((handle, _)) = started {
                handle.cancel();
            }
            return Vec::new();
        }
        let Some(window) = self.tools.usage.as_mut() else {
            return Vec::new();
        };
        match message {
            UsageMsg::Started(generation, started) => match started {
                Ok((handle, choices)) => {
                    window.choices = choices;
                    window.handle = Some(handle.clone());
                    window.stand = Stand::Running;
                    window.refresh();
                    return vec![Effect::Task(ticker(handle, generation))];
                }
                Err(code) => window.stand = Stand::Failed(error_text(code)),
            },
            UsageMsg::Tick(generation) if generation == window.generation => window.refresh(),
            UsageMsg::Tick(_) => {}
            UsageMsg::Tab(tab) => {
                window.tab = tab;
                window.refresh();
            }
            UsageMsg::Click(index) => window.list.point_at(index, window.rows.len()),
            UsageMsg::Open(index) => {
                window.list.point_at(index, window.rows.len());
                enter(window);
            }
            UsageMsg::Scroll(rows) => window.list.scroll(rows, window.rows.len()),
            UsageMsg::Viewport(rows) => window.list.set_page(rows, window.rows.len()),
            UsageMsg::FileClick(index) => window.files.point_at(index, window.largest.len()),
            UsageMsg::FileScroll(rows) => window.files.scroll(rows, window.largest.len()),
            UsageMsg::FileViewport(rows) => window.files.set_page(rows, window.largest.len()),
            UsageMsg::Up => up(window),
            UsageMsg::ShowInPanel => return self.usage_in_panel(),
            UsageMsg::Reread => return self.reread_usage(),
            UsageMsg::Stop => stop(window),
            UsageMsg::Close => self.close_disk_usage(),
            UsageMsg::Feed => return self.feed_largest(),
        }
        Vec::new()
    }

    /// A key while the window has the keyboard: the arrows in the list, Enter into a folder,
    /// Backspace up, Ctrl+Enter to the panel, Ctrl+R again, Escape stops the walk or closes.
    pub(super) fn disk_usage_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        if !captured && let Some(effects) = field_walk::walk_key(chord) {
            return effects;
        }
        let Some(window) = self.tools.usage.as_mut() else {
            return Vec::new();
        };
        if chord.ctrl && !chord.alt && !chord.shift {
            return match chord.key.as_str() {
                "Enter" => self.usage_in_panel(),
                "r" => self.reread_usage(),
                _ => Vec::new(),
            };
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        let folders = window.tab == UsageTab::Folders;
        match chord.key.as_str() {
            "Escape" if window.running() => stop(window),
            "Escape" => self.close_disk_usage(),
            _ if captured => {}
            "Enter" if folders => enter(window),
            "Backspace" if folders => up(window),
            key => {
                let (list, len) = if folders {
                    (&mut window.list, window.rows.len())
                } else {
                    (&mut window.files, window.largest.len())
                };
                let page = list.page() as isize - 1;
                match key {
                    "ArrowUp" => list.step(-1, len),
                    "ArrowDown" => list.step(1, len),
                    "PageUp" => list.step(-page, len),
                    "PageDown" => list.step(page, len),
                    _ => {}
                }
            }
        }
        Vec::new()
    }

    /// The window closes; a walk still running stops.
    fn close_disk_usage(&mut self) {
        if let Some(window) = self.tools.usage.take() {
            stop_walk(&window);
        }
    }

    /// Ctrl+R: the root read again from the start, the window on the same folder's root.
    fn reread_usage(&mut self) -> Vec<Effect> {
        let Some(window) = self.tools.usage.as_ref() else {
            return Vec::new();
        };
        stop_walk(window);
        let (panel, root) = (window.panel, window.root.clone());
        let generation = self.tools.next_usage();
        self.tools.usage = Some(DiskUsageWindow::new(panel, root.clone(), generation));
        let path = Some(root.path.display().to_string());
        vec![Effect::Task(start_at(
            self.engine.clone(),
            panel,
            path,
            generation,
        ))]
    }

    /// Ctrl+Enter: the active panel shows the folder shown, the cursor on the row's folder.
    fn usage_in_panel(&mut self) -> Vec<Effect> {
        let Some(window) = self.tools.usage.as_ref() else {
            return Vec::new();
        };
        let folder = if window.relative.is_empty() {
            window.root.clone()
        } else {
            window.root.join(&window.relative)
        };
        let mut commands = vec![Command::Navigate {
            panel: Some(self.active),
            path: rc_engine::shown_path(&folder).display().to_string(),
        }];
        if let Some(row) = window.current().filter(|row| row.node.is_some()) {
            commands.push(Command::SetCursor {
                panel: Some(self.active),
                target: CursorTarget::Name(row.name.clone()),
            });
        }
        vec![Effect::Dispatch(self.active, commands)]
    }

    /// "Feed to listbox": the largest files flat in the panel the window belongs to.
    fn feed_largest(&mut self) -> Vec<Effect> {
        let Some(window) = self.tools.usage.as_ref().filter(|w| !w.largest.is_empty()) else {
            return Vec::new();
        };
        let panel = window.panel;
        let feed = Command::FeedPaths {
            panel: Some(panel),
            root: Some(rc_engine::shown_path(&window.root).display().to_string()),
            paths: window
                .largest
                .iter()
                .map(|file| file.path.clone())
                .collect(),
            groups: Vec::new(),
        };
        vec![Effect::Dispatch(panel, vec![feed])]
    }
}

/// Enter: into the folder under the cursor.
fn enter(window: &mut DiskUsageWindow) {
    if let Some(node) = window.current().and_then(|row| row.node) {
        window.show(node, None);
    }
}

/// Backspace: up to the parent of the folder shown, the cursor on the folder left.
fn up(window: &mut DiskUsageWindow) {
    let parent = window.handle.as_ref().and_then(|handle| {
        handle.read(|tree| tree.node(window.folder).and_then(|node| node.parent))
    });
    if let Some(parent) = parent {
        let from = window.folder;
        window.show(parent, Some(from));
    }
}

/// Escape while the walk runs: it stops, what it found stays.
fn stop(window: &mut DiskUsageWindow) {
    stop_walk(window);
    window.refresh();
}

fn stop_walk(window: &DiskUsageWindow) {
    if let Some(handle) = &window.handle {
        handle.cancel();
    }
}

/// The walk of the panel's folder, started off the window's thread.
fn start(engine: Engine, panel: PanelId, generation: u64) -> Task<Message> {
    start_at(engine, panel, None, generation)
}

fn start_at(
    engine: Engine,
    panel: PanelId,
    path: Option<String>,
    generation: u64,
) -> Task<Message> {
    Task::perform(
        async move {
            let choices = engine.disk_usage_choices().await;
            engine
                .start_disk_usage(Some(panel), path.as_deref(), choices.largest)
                .map(|handle| (handle, choices))
                .map_err(|error| error.code())
        },
        move |started| wrap(UsageMsg::Started(generation, started)),
    )
}

/// A tick every [`REFRESH_EVERY`] until the walk ended, then one more.
fn ticker(handle: DiskUsageHandle, generation: u64) -> Task<Message> {
    let ticks = stream::unfold(Some(handle), move |handle| async move {
        let handle = handle?;
        tokio::time::sleep(REFRESH_EVERY).await;
        let next = (!handle.finished()).then_some(handle);
        Some((generation, next))
    })
    .boxed();
    Task::run(ticks, |generation| wrap(UsageMsg::Tick(generation)))
}

#[cfg(test)]
#[path = "disk_usage_tests.rs"]
mod tests;
