//! File icons in the list (RC-010-10): off, generic, or native - Explorer's icons on Windows.
//!
//! A row shows the generic icon at once and the native one as soon as it has loaded. After every
//! message [`App::request_icons`] collects the keys of the rows on screen - one screen, whatever
//! the folder's length - and sends those not yet known to the loader thread (`loader.rs`); its
//! answer comes back as [`IconsMsg::Loaded`]. Icons are cached by extension, per file only for
//! the kinds with their own (`key.rs`), in a bounded LRU (`cache.rs`). The generic icons are
//! drawn in code (`fallback.rs`); Windows' shell is `windows.rs`, the one module with `unsafe`.
//! Off Windows there is no native provider: "native" shows the generic icons.

mod cache;
mod fallback;
mod key;
mod loader;
mod pixels;
#[cfg(windows)]
mod windows;

use std::{iter, ops::Range, sync::Arc};

use iced::{
    Alignment, Element, Length, Task,
    widget::{
        container,
        image::{FilterMethod, Handle, Image},
        stack, text,
    },
};
use rc_engine::{
    Entry, PanelView, VfsPath,
    settings::{FILE_ICONS_KEY, stored_setting},
};

use self::{
    cache::IconCache,
    fallback::Glyph,
    key::IconKey,
    loader::{Answers, IconProvider, Loaded, Loader},
};
use super::{App, Effect, Message, panel::Pane};

/// The side of a row's icon, in logical pixels.
pub const ICON_SIZE: f32 = 16.0;
/// How strongly a hidden or system entry's icon shows under its `!`.
const HIDDEN_OPACITY: f32 = 0.55;
/// The native icons kept: every extension seen and a few screens of per-file icons, at most
/// 4 KiB each at 200 %.
const CAPACITY: usize = 1024;

/// The setting `appearance.file_icons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconMode {
    Off,
    Generic,
    #[default]
    Native,
}

impl IconMode {
    /// The mode a stored value names; native for anything else.
    pub fn from_setting(value: &str) -> Self {
        match value {
            "off" => Self::Off,
            "generic" => Self::Generic,
            _ => Self::Native,
        }
    }
}

#[derive(Debug, Clone)]
pub enum IconsMsg {
    /// The setting, read at the start and after every change.
    Mode(IconMode),
    /// The loader's answers to one batch.
    Loaded(Answers),
}

/// The icons of both panels.
pub struct Icons {
    mode: IconMode,
    /// The look the generic icons are drawn for.
    dark: bool,
    file: Handle,
    folder: Handle,
    /// The native icons loaded, by key.
    cache: IconCache<Handle>,
    loader: Option<Loader>,
    /// No native icons here: the platform has none, or the loader thread did not start.
    unavailable: bool,
    /// The rows on screen when their icons were last asked for; asked again only once they
    /// change, or the icons known do (RC-080-45).
    asked: Option<Vec<Shown>>,
}

/// One file list's rows on screen: its listing, its folder and the rows drawn.
#[derive(Debug)]
struct Shown {
    entries: Arc<[Entry]>,
    dir: VfsPath,
    rows: Range<usize>,
}

impl Shown {
    fn of(pane: &Pane) -> Self {
        Self {
            entries: Arc::clone(&pane.state.entries),
            dir: pane.state.path.clone(),
            rows: pane.visible_range(),
        }
    }

    /// Whether `pane` shows these rows still. The listing is held, so a new one is never
    /// taken for it at the same address.
    fn still(&self, pane: &Pane) -> bool {
        Arc::ptr_eq(&self.entries, &pane.state.entries)
            && self.rows == pane.visible_range()
            && self.dir == pane.state.path
    }
}

/// The file lists among `panes`, whose rows have icons.
fn lists(panes: &[Pane]) -> impl Iterator<Item = &Pane> {
    panes
        .iter()
        .filter(|pane| pane.state.view == PanelView::Files)
}

impl Default for Icons {
    fn default() -> Self {
        Self {
            mode: IconMode::default(),
            dark: false,
            file: generic(Glyph::File, false),
            folder: generic(Glyph::Folder, false),
            cache: IconCache::new(CAPACITY),
            loader: None,
            unavailable: !cfg!(windows),
            asked: None,
        }
    }
}

impl Icons {
    /// The icon of a row - `entry`, listed in `dir`, or `..` for `None`; `None` while icons are
    /// off. The generic icon until the native one has loaded.
    pub fn of(&self, dir: &VfsPath, entry: Option<&Entry>) -> Option<Handle> {
        if self.mode == IconMode::Off {
            return None;
        }
        if self.native() {
            let key = entry.map_or(IconKey::Folder, |entry| IconKey::of(entry, dir));
            if let Some(icon) = self.cache.get(&key) {
                return Some(icon.clone());
            }
        }
        Some(if entry.is_none_or(Entry::is_dir_like) {
            self.folder.clone()
        } else {
            self.file.clone()
        })
    }

    fn native(&self) -> bool {
        self.mode == IconMode::Native && !self.unavailable
    }

    fn set_mode(&mut self, mode: IconMode) {
        self.mode = mode;
        self.asked = None;
        if !self.native() {
            self.cache.clear();
        }
    }

    /// A change of look draws the generic icons anew and loads the native ones again.
    fn set_dark(&mut self, dark: bool) {
        if dark == self.dark {
            return;
        }
        self.dark = dark;
        self.asked = None;
        self.file = generic(Glyph::File, dark);
        self.folder = generic(Glyph::Folder, dark);
        self.cache.clear();
    }

    /// Marks `keys` as shown; the task that loads those not known yet, if any.
    fn request(&mut self, keys: impl Iterator<Item = IconKey>) -> Option<Task<Message>> {
        if !self.native() {
            return None;
        }
        let fresh: Vec<IconKey> = keys.filter_map(|key| self.cache.want(key)).collect();
        if fresh.is_empty() {
            return None;
        }
        if self.loader.is_none() {
            self.loader = native_loader();
        }
        let Some(answers) = self
            .loader
            .as_ref()
            .and_then(|loader| loader.request(fresh))
        else {
            // No thread: the generic icons for the rest of the run.
            self.unavailable = true;
            self.loader = None;
            self.cache.clear();
            return None;
        };
        // A thread that died with the batch leaves its keys pending: they keep the generic icon.
        let task = Task::perform(
            async move { answers.await.unwrap_or_default() },
            |answers| Message::Icons(IconsMsg::Loaded(answers)),
        );
        Some(task)
    }

    fn answered(&mut self, answers: Answers) {
        // A key an older batch skipped is to be asked again.
        self.asked = None;
        for (key, loaded) in answers {
            match loaded {
                Loaded::Icon(icon) => self.cache.fill(&key, Some(icon)),
                Loaded::None => self.cache.fill(&key, None),
                Loaded::Skipped => self.cache.forget(&key),
            }
        }
    }
}

/// The generic icon `glyph` for the light or the dark look.
fn generic(glyph: Glyph, dark: bool) -> Handle {
    let icon = fallback::draw(glyph, dark);
    Handle::from_rgba(icon.width, icon.height, icon.pixels)
}

/// The loader thread with the platform's provider.
fn native_loader() -> Option<Loader> {
    Loader::spawn(provider)
}

#[cfg(windows)]
fn provider() -> Box<dyn IconProvider> {
    Box::new(windows::ShellIcons::start())
}

/// Off Windows no loader starts (`Icons::unavailable`); were one asked for, it has no icons.
#[cfg(not(windows))]
fn provider() -> Box<dyn IconProvider> {
    struct NoIcons;
    impl IconProvider for NoIcons {
        fn load(&self, _key: &IconKey) -> Option<pixels::Rgba> {
            None
        }
    }
    Box::new(NoIcons)
}

/// A row's icon cell; a hidden or system entry's icon faded with a `!` over it, as Total
/// Commander marks them (owner, 2026-10-19).
pub fn view<'a>(icon: Handle, hidden: bool) -> Element<'a, Message> {
    let image = Image::<Handle>::new(icon)
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .filter_method(FilterMethod::Linear);
    if !hidden {
        return image.into();
    }
    let mark = text("!")
        .size(ICON_SIZE * 0.8)
        .font(crate::theme::UI_FONT_STRONG)
        .color(crate::theme::DANGER);
    stack![
        image.opacity(HIDDEN_OPACITY),
        container(mark)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    ]
    .width(ICON_SIZE)
    .height(ICON_SIZE)
    .into()
}

impl App {
    /// After every message that changed the rows on screen: the icons of those rows that are
    /// not known yet go to the loader. One screen per panel, so a folder of 100 000 entries costs
    /// what a short one does; a message that leaves the rows as they were costs nothing.
    pub(super) fn request_icons(&mut self) -> Task<Message> {
        self.icons.set_dark(self.dark);
        let Self { panes, icons, .. } = self;
        let panes: &[Pane] = &panes[..];
        let unchanged = icons.asked.as_ref().is_some_and(|asked| {
            asked.len() == lists(panes).count()
                && asked
                    .iter()
                    .zip(lists(panes))
                    .all(|(shown, pane)| shown.still(pane))
        });
        if unchanged {
            return Task::none();
        }
        icons.asked = Some(lists(panes).map(Shown::of).collect());
        let keys = lists(panes)
            .flat_map(|pane| {
                let dir = &pane.state.path;
                pane.visible_range()
                    .filter_map(move |row| pane.entry_index(row))
                    .filter_map(move |index| pane.entries().get(index))
                    .map(move |entry| IconKey::of(entry, dir))
            })
            // The `..` rows.
            .chain(iter::once(IconKey::Folder));
        icons.request(keys).unwrap_or_else(Task::none)
    }

    pub(super) fn on_icons(&mut self, message: IconsMsg) -> Vec<Effect> {
        match message {
            IconsMsg::Mode(mode) => self.icons.set_mode(mode),
            IconsMsg::Loaded(answers) => self.icons.answered(answers),
        }
        Vec::new()
    }

    /// Reads the icon setting: at the start, and when the engine reports it changed.
    pub(super) fn load_icon_mode(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let value = stored_setting(&db, FILE_ICONS_KEY).await;
                value
                    .as_ref()
                    .and_then(|value| value.as_str())
                    .map_or_else(IconMode::default, IconMode::from_setting)
            },
            |mode| Message::Icons(IconsMsg::Mode(mode)),
        )
    }
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
