//! The thumbnails view (RC-100-89, Ctrl+Shift+F1, Total Commander's `cm_SrcThumbs`): a grid of
//! tiles, a picture's thumbnail or an entry's icon over each name (`file_list/grid.rs`); the
//! panel's steps through it are `panel/grid.rs`.
//!
//! Tiles load as the icons do: after every message [`App::request_thumbnails`] collects the tiles
//! on screen and one line below - one screen, whatever the folder's length - and starts the
//! loads of those not known yet; at most two run at once (`load.rs`), and one whose tile left the
//! screen before its turn is skipped. A tile comes from the disk cache (`disk.rs`), from
//! rCommander's own decoder with the Lister's limits (`decode.rs`), or from Explorer
//! (`explorer.rs`, Windows, `thumbnails.explorer`); it is kept in a bounded LRU of 64 MiB
//! (`cache.rs`) by the file's path, size and time and the tile size (`key.rs`). A file on a
//! server, a device or in an archive gets one only with `thumbnails.remote`, and its tile never
//! goes to disk. The cache belongs to the window: "Clear cache" on the settings' Thumbnails tab
//! is no engine command.

mod cache;
mod decode;
mod disk;
mod explorer;
mod key;
mod load;
mod settings;

use std::{collections::HashSet, ops::Range, sync::Arc};

use iced::{Size, Task, mouse::ScrollDelta, widget::image::Handle};
use rc_engine::{Command, Entry, PanelId, PanelView, VfsPath, ZoomStep};
use tokio::sync::Semaphore;

pub use self::settings::{ThumbSettings, concerns};
use self::{
    cache::{MEMORY_BYTES, TileCache},
    disk::DiskCache,
    key::ThumbKey,
    load::{AT_ONCE, Job, Loaded, Wanted},
};
use super::{App, Effect, Message, panel::Pane};
use crate::{recover::LogPanic as _, theme};

/// The room around a tile's picture, in logical pixels.
pub const TILE_PAD: f32 = 6.0;

#[derive(Debug, Clone)]
pub enum ThumbsMsg {
    /// The settings, read at the start and after every change.
    Settings(ThumbSettings),
    /// One tile's load ended.
    Loaded(ThumbKey, Loaded),
    /// "Clear cache" on the settings' Thumbnails tab.
    Clear,
    /// The bytes the disk cache holds, measured.
    Measured(u64),
}

/// The tiles of both panels.
pub struct Thumbnails {
    settings: ThumbSettings,
    /// Read once from the database: until then nothing loads.
    ready: bool,
    cache: TileCache<Handle>,
    wanted: Wanted,
    gate: Arc<Semaphore>,
    /// The tiles on screen when they were last asked for; asked again only once they change.
    asked: Option<Vec<Shown>>,
}

impl Default for Thumbnails {
    fn default() -> Self {
        Self {
            settings: ThumbSettings::default(),
            ready: false,
            cache: TileCache::new(MEMORY_BYTES),
            wanted: Wanted::default(),
            gate: Arc::new(Semaphore::new(AT_ONCE)),
            asked: None,
        }
    }
}

/// One grid's tiles on screen: its listing, its folder, the rows asked for, the tile size.
#[derive(Debug)]
struct Shown {
    entries: Arc<[Entry]>,
    dir: VfsPath,
    rows: Range<usize>,
    size: u32,
}

impl Shown {
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.entries, &other.entries)
            && self.rows == other.rows
            && self.size == other.size
            && self.dir == other.dir
    }
}

/// The panes that show the thumbnails.
fn grids(panes: &[Pane]) -> impl Iterator<Item = &Pane> {
    panes
        .iter()
        .filter(|pane| pane.state.view == PanelView::Files && pane.thumbnails())
}

/// The rows of `pane` whose tiles are asked for: those on screen and one line below.
fn asked_rows(pane: &Pane) -> Range<usize> {
    let shown = pane.visible_range();
    shown.start..(shown.end + pane.columns()).min(pane.rows())
}

impl Thumbnails {
    /// The tile of `entry`, listed in `dir`, once it has loaded; `None` shows its icon.
    pub fn of(&self, dir: &VfsPath, entry: &Entry) -> Option<Handle> {
        key::source(entry, dir, &self.settings)?;
        let key = ThumbKey::of(entry, dir, self.settings.size);
        self.cache.get(&key).cloned()
    }

    /// The tile's side, in pixels.
    pub fn size(&self) -> u32 {
        self.settings.size
    }

    /// The room of one tile in the grid: its picture with [`TILE_PAD`] around it and the name
    /// under it in the list's font.
    pub fn cell(&self) -> Size {
        let side = self.settings.size as f32 + 2.0 * TILE_PAD;
        Size::new(side, side + theme::fonts::list().row_height())
    }

    /// The disk cache, while it is on.
    fn disk(&self) -> Option<DiskCache> {
        self.settings.disk_cache.then(DiskCache::user).flatten()
    }
}

impl App {
    /// After every message that changed the tiles on screen: the loads of those not known yet.
    /// One screen per panel; a message that leaves them as they were costs nothing.
    pub(super) fn request_thumbnails(&mut self) -> Task<Message> {
        let Self {
            panes,
            thumbnails,
            engine,
            ..
        } = self;
        if !thumbnails.ready {
            return Task::none();
        }
        let size = thumbnails.settings.size;
        let shown: Vec<Shown> = grids(&panes[..])
            .map(|pane| Shown {
                entries: Arc::clone(&pane.state.entries),
                dir: pane.state.path.clone(),
                rows: asked_rows(pane),
                size,
            })
            .collect();
        let unchanged = thumbnails.asked.as_ref().is_some_and(|asked| {
            asked.len() == shown.len() && asked.iter().zip(&shown).all(|(a, b)| a.same(b))
        });
        if unchanged {
            return Task::none();
        }
        thumbnails.asked = Some(shown);
        let mut on_screen = HashSet::new();
        let mut fresh = Vec::new();
        for pane in grids(&panes[..]) {
            let dir = &pane.state.path;
            for index in asked_rows(pane).filter_map(|row| pane.entry_index(row)) {
                let Some(entry) = pane.entries().get(index) else {
                    continue;
                };
                let Some(source) = key::source(entry, dir, &thumbnails.settings) else {
                    continue;
                };
                let key = ThumbKey::of(entry, dir, size);
                if thumbnails.cache.want(&key) {
                    fresh.push((key.clone(), source));
                }
                on_screen.insert(key);
            }
        }
        thumbnails.wanted.set(on_screen);
        let disk = thumbnails.disk();
        let tasks = fresh.into_iter().map(|(key, source)| {
            let job = Job {
                key: key.clone(),
                source,
                engine: engine.clone(),
                disk: disk.clone(),
                wanted: thumbnails.wanted.clone(),
                gate: Arc::clone(&thumbnails.gate),
            };
            Task::perform(load::load(job), move |loaded| {
                Message::Thumbnails(ThumbsMsg::Loaded(key, loaded))
            })
        });
        Task::batch(tasks)
    }

    pub(super) fn on_thumbnails(&mut self, message: ThumbsMsg) -> Vec<Effect> {
        if let ThumbsMsg::Measured(bytes) = message {
            if let Some(dialog) = self.settings.as_mut() {
                dialog.set_thumbnail_cache(bytes);
            }
            return Vec::new();
        }
        let thumbnails = &mut self.thumbnails;
        match message {
            ThumbsMsg::Settings(settings) => {
                let first = !thumbnails.ready;
                let changed = settings != thumbnails.settings;
                thumbnails.ready = true;
                thumbnails.settings = settings;
                if changed {
                    // Another size, source or switch: every tile is asked for again.
                    thumbnails.cache.clear();
                }
                thumbnails.asked = None;
                if first || changed {
                    return vec![Effect::Task(trim_task(thumbnails.disk(), &settings))];
                }
            }
            ThumbsMsg::Loaded(key, loaded) => {
                // A key an older screen skipped is to be asked again.
                thumbnails.asked = None;
                match loaded {
                    Loaded::Tile(tile, bytes) => thumbnails.cache.fill(&key, Some((tile, bytes))),
                    Loaded::None => thumbnails.cache.fill(&key, None),
                    Loaded::Skipped => thumbnails.cache.forget(&key),
                }
            }
            ThumbsMsg::Clear => {
                thumbnails.cache.clear();
                thumbnails.asked = None;
                let task = Task::perform(
                    blocking_bytes(|| DiskCache::user().map_or(0, |disk| disk.clear())),
                    |bytes| Message::Thumbnails(ThumbsMsg::Measured(bytes)),
                );
                return vec![Effect::Task(task)];
            }
            ThumbsMsg::Measured(_) => {}
        }
        Vec::new()
    }

    /// Reads the settings: at the start, and when the engine reports one changed.
    pub(super) fn thumbnail_settings_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(async move { ThumbSettings::read(&db).await }, |settings| {
            Message::Thumbnails(ThumbsMsg::Settings(settings))
        })
    }

    /// The bytes the disk cache holds, for the settings' Thumbnails tab.
    pub(super) fn thumbnail_cache_task() -> Task<Message> {
        Task::perform(
            blocking_bytes(|| DiskCache::user().map_or(0, |disk| disk.size())),
            |bytes| Message::Thumbnails(ThumbsMsg::Measured(bytes)),
        )
    }

    /// The list's measured size: the rows that fit, or the lines and tiles of the thumbnails;
    /// the window's and the engine's paging both take them.
    pub(super) fn viewport(&mut self, panel: PanelId, size: Size) {
        let cell = self.thumbnails.cell();
        let pane = self.pane_mut(panel);
        let (rows, tiles) = if pane.thumbnails() {
            // The scroll bar and the list's padding take their part of the width.
            let room = size.width - super::file_list::GRID_CHROME;
            (size.height / cell.height, (room / cell.width).max(1.0))
        } else {
            (size.height / theme::fonts::list().row_height(), 1.0)
        };
        let (rows, tiles) = (rows.floor() as usize, tiles.floor() as usize);
        pane.set_width(size.width);
        pane.set_tiles(tiles);
        pane.set_page_rows(rows);
        self.engine.set_page_rows(panel, rows);
        self.engine.set_page_tiles(panel, tiles);
    }

    /// The mouse wheel over a list: it scrolls the view; over the thumbnails with Ctrl it makes
    /// the tiles larger (away from the person) or smaller, as Total Commander does.
    pub(super) fn wheel(&mut self, panel: PanelId, delta: ScrollDelta) -> Vec<Effect> {
        if !self.pane(panel).thumbnails() {
            let rows = theme::wheel::rows(delta, theme::fonts::list().row_height());
            self.pane_mut(panel).scroll(rows);
            return Vec::new();
        }
        let lines = theme::wheel::rows(delta, self.thumbnails.cell().height);
        if !self.modifiers_held().ctrl {
            self.pane_mut(panel).scroll(lines);
            return Vec::new();
        }
        let step = match lines.signum() {
            0 => return Vec::new(),
            -1 => ZoomStep::In,
            _ => ZoomStep::Out,
        };
        let zoom = Command::ThumbnailZoom { step };
        vec![Effect::Dispatch(panel, vec![zoom])]
    }
}

/// The start's trim of the disk cache to its size, and what it holds after; nothing while the
/// cache is off.
fn trim_task(disk: Option<DiskCache>, settings: &ThumbSettings) -> Task<Message> {
    let limit = settings.cache_bytes();
    Task::perform(
        blocking_bytes(move || disk.map_or(0, |disk| disk.trim(limit))),
        |bytes| Message::Thumbnails(ThumbsMsg::Measured(bytes)),
    )
}

/// `work` on a blocking thread; 0 when it panicked (logged).
async fn blocking_bytes(work: impl FnOnce() -> u64 + Send + 'static) -> u64 {
    tokio::task::spawn_blocking(work)
        .await
        .log_panic("app.thumbnail_cache")
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "thumbnails_tests.rs"]
mod tests;
