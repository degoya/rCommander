//! The internal viewer (RC-020-07): Lister on F3 (`view.open`), Quick View on Ctrl+Q
//! (`panel.toggle_quick_view`), as Total Commander has them.
//!
//! The engine names the file and announces it; this module reads it through the engine's
//! GUI-free viewer model (`rc_engine::viewer`), a page at a time and off the UI thread. While
//! `appearance.dialog_windows` is on each Lister is a window of its own (RC-080-15,
//! `window.rs`); else Lister covers the main window, one over the other when several are open,
//! and Escape closes the top one. While a Lister has the keys they are its own (`keys.rs`), not
//! the keymap's.
//!
//! - `lister.rs` - one file's view: mode, encoding, place, page; `find.rs` - its search bar.
//! - `load.rs` - the reads behind it; `picture.rs` - decoding a picture within limits.
//! - `quick.rs` - Quick View's state; `draw.rs` - drawing both; `window.rs` - Lister's window.
//! - `markdown.rs` - a Markdown file's preview, `markdown_view.rs` drawing it, `links.rs` its
//!   links (RC-020-09); `text_size.rs` - Ctrl+wheel, Ctrl++, Ctrl+- and Ctrl+0 (RC-020-12).
//! - `document.rs` - a plugin's document with several views (RC-100-10): `spans.rs` its coloured
//!   text, `tree.rs` its tree, `views.rs` switching them, `tree_view.rs` drawing the tree and the
//!   list of views; `run.rs` - a lister's requests as tasks; `plugin.rs` - a file opened with its
//!   viewer plugin, or built in (RC-100-13).

pub(in crate::app) mod document;
mod draw;
mod find;
mod keys;
mod last_page;
mod links;
mod lister;
mod load;
mod markdown;
mod markdown_view;
mod output;
mod picture;
mod plugin;
mod quick;
mod run;
mod spans;
mod text_size;
mod tree;
mod tree_view;
mod views;
mod window;

use iced::{
    Size, Task,
    widget::{Id, operation, text_editor::Action},
};
use rc_engine::{
    Chord, Command, CursorTarget, EngineEvent, Outcome, PanelId, VfsPath,
    settings::VIEWER_TOKEN_COLORS_KEY,
};

use self::{
    document::DocumentMessage,
    find::FindOption,
    keys::ListerKey,
    lister::Lister,
    load::{Found, Opened, Page, Picture},
    markdown::PreviewMessage,
    plugin::{PluginOpened, Plugins},
    quick::{DirSummary, Quick, QuickShown},
    text_size::{TextSizeMsg, TextZoom},
    views::TokenColors,
};
pub(super) use self::{
    draw::{editor_style, read_only},
    text_size::wheel_events,
};
use super::{App, Effect, Message};
use crate::i18n;

/// The open viewers and the Quick View.
#[derive(Debug, Default)]
pub struct Viewers {
    /// Lister windows, the top one last.
    pub open: Vec<Lister>,
    pub quick: Option<Quick>,
    /// What the settings make of the text size keys (RC-020-12).
    pub text_zoom: TextZoom,
    /// `plugins.viewer_token_colors` (RC-100-10).
    pub token_colors: TokenColors,
    /// The viewer plugins, loaded when first needed (RC-100-13).
    plugins: Plugins,
    next_id: u64,
}

impl Viewers {
    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn lister_mut(&mut self, id: u64) -> Option<&mut Lister> {
        if let Some(Quick {
            shown: QuickShown::File(lister),
            ..
        }) = &mut self.quick
            && lister.id == id
        {
            return Some(&mut **lister);
        }
        self.open.iter_mut().find(|lister| lister.id == id)
    }

    fn lister(&self, id: u64) -> Option<&Lister> {
        if let Some(Quick {
            shown: QuickShown::File(lister),
            ..
        }) = &self.quick
            && lister.id == id
        {
            return Some(&**lister);
        }
        self.open.iter().find(|lister| lister.id == id)
    }
}

#[derive(Debug, Clone)]
pub enum ViewerMessage {
    Opened(u64, Result<Opened, &'static str>),
    Reloaded(u64, Result<Opened, &'static str>),
    /// A page for a lister, by the generation it was asked under.
    Page(u64, u64, Result<Page, &'static str>),
    /// The file's picture for a lister, by the generation it was asked under.
    Picture(u64, u64, Result<Picture, &'static str>),
    Found(u64, Found),
    Editor(u64, Action),
    Viewport(u64, Size),
    Scrollbar(u64, f32),
    BarInput(u64, String),
    BarSubmit(u64),
    Toggle(u64, FindOption),
    Summary(VfsPath, Result<DirSummary, &'static str>),
    Preview(u64, PreviewMessage),
    TextSize(TextSizeMsg),
    /// A plugin document's answers and inputs (RC-100-10).
    Document(u64, DocumentMessage),
    /// `plugins.viewer_token_colors` read.
    TokenColors(bool),
    /// What the viewer plugin made of the file (RC-100-13).
    Plugin(u64, PluginOpened),
}

fn wrap(message: ViewerMessage) -> Message {
    Message::Viewer(message)
}

impl App {
    /// The viewer's share of a message, before anything else sees it: the engine's requests
    /// for a viewer, the keys while Lister is open or Quick View has them, and the cursor
    /// moves Quick View follows. `None` for a message the viewer leaves alone.
    pub(super) fn viewer_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::ViewFile { panel, path }) => {
                Some(self.open_lister(*panel, path.clone()))
            }
            Message::Engine(EngineEvent::QuickView { panel }) => Some(self.toggle_quick(*panel)),
            Message::Engine(EngineEvent::SettingChanged { key }) if text_size::concerns(key) => {
                Some(vec![Effect::Task(self.text_zoom_task())])
            }
            Message::Engine(EngineEvent::SettingChanged { key })
                if key == VIEWER_TOKEN_COLORS_KEY =>
            {
                Some(vec![Effect::Task(self.token_colors_task())])
            }
            Message::Engine(EngineEvent::SettingChanged { key }) if plugin::concerns(key) => {
                // Loaded again for the next file; documents open keep their sessions.
                self.viewers.plugins = Plugins::default();
                None
            }
            Message::Engine(EngineEvent::PluginsChanged) => {
                // Installed, switched, rolled back or resumed (RC-100-09): the same.
                self.viewers.plugins = Plugins::default();
                None
            }
            Message::Engine(event @ EngineEvent::Panel { panel, .. })
                if self
                    .viewers
                    .quick
                    .as_ref()
                    .is_some_and(|q| q.source == *panel) =>
            {
                let mut effects = self.on_engine(event.clone());
                effects.extend(self.quick_follow());
                Some(effects)
            }
            Message::Key(chord) | Message::KeyCaptured(chord) if self.lister_over_main() => {
                let captured = matches!(message, Message::KeyCaptured(_));
                let top = self.viewers.open.last().map(|lister| lister.id);
                Some(top.map_or_else(Vec::new, |id| self.lister_key(id, chord, captured)))
            }
            Message::Press if self.lister_over_main() => Some(Vec::new()),
            Message::Key(chord) if self.views_parent_row(chord) => Some(Vec::new()),
            Message::Key(chord)
                if self.viewers.quick.is_some()
                    && self.database.is_none()
                    && self.drive_bar.menu.is_none() =>
            {
                if let Some(effects) = self.quick_key(chord) {
                    return Some(effects);
                }
                // The panel's own key, then Quick View catches up: a step onto or off the `..`
                // row moves no cursor in the engine.
                let mut effects = match self.terminal_key(chord) {
                    Some(effects) => effects,
                    None => self.on_key(chord.clone(), false),
                };
                effects.extend(self.quick_follow());
                Some(effects)
            }
            _ => None,
        }
    }

    /// F3 on the `..` row views nothing; the engine's cursor stands on the first entry there.
    fn views_parent_row(&self, chord: &Chord) -> bool {
        let pane = self.pane(self.active);
        self.prompt.is_none()
            && self.database.is_none()
            && self.drive_bar.menu.is_none()
            && pane.on_parent()
            && matches!(
                self.keymap.lookup(chord, pane.state.view),
                Some(Command::ViewFile { path: None, .. })
            )
    }

    /// A message of a viewer.
    pub(super) fn on_viewer(&mut self, message: ViewerMessage) -> Vec<Effect> {
        if let Some(effects) = self.editor_sizes_text(&message) {
            return effects;
        }
        let id = match &message {
            ViewerMessage::TextSize(message) => return self.on_text_size(message.clone()),
            ViewerMessage::TokenColors(on) => return self.on_token_colors(*on),
            ViewerMessage::Plugin(id, opened) => return self.on_plugin(*id, opened.clone()),
            ViewerMessage::Preview(id, _) | ViewerMessage::Document(id, _) => *id,
            ViewerMessage::Summary(path, summary) => {
                if let Some(Quick {
                    shown:
                        QuickShown::Dir {
                            path: shown,
                            summary: slot,
                        },
                    ..
                }) = &mut self.viewers.quick
                    && shown == path
                {
                    *slot = Some(summary.map_err(i18n::error_text));
                }
                return Vec::new();
            }
            ViewerMessage::Opened(id, _)
            | ViewerMessage::Reloaded(id, _)
            | ViewerMessage::Page(id, ..)
            | ViewerMessage::Picture(id, ..)
            | ViewerMessage::Found(id, _)
            | ViewerMessage::Editor(id, _)
            | ViewerMessage::Viewport(id, _)
            | ViewerMessage::Scrollbar(id, _)
            | ViewerMessage::BarInput(id, _)
            | ViewerMessage::BarSubmit(id)
            | ViewerMessage::Toggle(id, _) => *id,
        };
        // An answer for a viewer closed meanwhile goes nowhere.
        let Some(lister) = self.viewers.lister_mut(id) else {
            return Vec::new();
        };
        let requests = match message {
            ViewerMessage::Preview(_, message) => return self.on_preview(id, message),
            ViewerMessage::Opened(_, Ok(opened)) => lister.opened(opened),
            ViewerMessage::Reloaded(_, Ok(opened)) => lister.reloaded(opened),
            ViewerMessage::Opened(_, Err(code)) | ViewerMessage::Reloaded(_, Err(code)) => {
                lister.failed(code);
                Vec::new()
            }
            ViewerMessage::Page(_, generation, page) => lister.paged(generation, page),
            ViewerMessage::Document(_, message) => lister.document_message(message),
            ViewerMessage::Picture(_, generation, picture) => {
                lister.picture_read(generation, picture);
                Vec::new()
            }
            ViewerMessage::Found(_, news) => lister.found(news),
            ViewerMessage::Editor(_, action) => lister.editor(action),
            ViewerMessage::Viewport(_, size) => {
                lister.set_rows(draw::rows_at(size.height, lister.line_height()))
            }
            ViewerMessage::Scrollbar(_, share) => lister.scrolled(share),
            ViewerMessage::BarInput(_, text) => {
                lister.bar_input(text);
                Vec::new()
            }
            ViewerMessage::BarSubmit(_) => lister.submit(),
            ViewerMessage::Toggle(_, option) => {
                lister.toggle(option);
                Vec::new()
            }
            ViewerMessage::Summary(..)
            | ViewerMessage::TextSize(_)
            | ViewerMessage::TokenColors(_)
            | ViewerMessage::Plugin(..) => Vec::new(),
        };
        self.run(id, requests)
    }

    /// F3: a new Lister on top.
    fn open_lister(&mut self, panel: PanelId, path: VfsPath) -> Vec<Effect> {
        let id = self.viewers.next_id();
        self.viewers.open.push(Lister::new(id, panel, path.clone()));
        vec![self.open_file(id, path)]
    }

    /// A key for the open Lister `id`; `captured` when the search bar's field took it, where
    /// only Escape counts.
    fn lister_key(&mut self, id: u64, chord: &Chord, captured: bool) -> Vec<Effect> {
        let Some(key) = keys::action(chord) else {
            return Vec::new();
        };
        if captured && key != ListerKey::Close {
            return Vec::new();
        }
        let Some(lister) = self.viewers.open.iter_mut().find(|lister| lister.id == id) else {
            return Vec::new();
        };
        match key {
            ListerKey::Close => {
                if lister.escape() {
                    self.viewers.open.retain(|lister| lister.id != id);
                }
                Vec::new()
            }
            ListerKey::NextFile => self.step_file(id, true),
            ListerKey::PreviousFile => self.step_file(id, false),
            _ => self.lister_does(id, key),
        }
    }

    /// A key for the lister `id`, and the bar's field focused when the key opened it.
    fn lister_does(&mut self, id: u64, key: ListerKey) -> Vec<Effect> {
        if let Some(effects) = self.text_size_key(id, key) {
            return effects;
        }
        let Some(lister) = self.viewers.lister_mut(id) else {
            return Vec::new();
        };
        let requests = lister.key(key);
        let bar_opened =
            lister.find.bar.is_some() && matches!(key, ListerKey::Search | ListerKey::GoTo);
        let mut effects = self.run(id, requests);
        if bar_opened {
            effects.push(Effect::Task(operation::focus(Id::new(draw::BAR_ID))));
        }
        effects
    }

    /// N and P: the next or previous file of the panel Lister was opened from, in the same
    /// Lister; the panel's cursor goes along, as in Total Commander.
    fn step_file(&mut self, id: u64, forward: bool) -> Vec<Effect> {
        let Some(lister) = self.viewers.lister(id) else {
            return Vec::new();
        };
        let panel = lister.panel;
        let pane = self.pane(panel);
        if lister.path.parent().as_ref() != Some(&pane.state.path) {
            return Vec::new();
        }
        let current = lister.name();
        let entries = pane.entries();
        let Some(at) = entries.iter().position(|entry| entry.name == current) else {
            return Vec::new();
        };
        let next = if forward {
            entries[at + 1..].iter().find(|entry| !entry.is_dir_like())
        } else {
            entries[..at]
                .iter()
                .rev()
                .find(|entry| !entry.is_dir_like())
        };
        let Some(next) = next else {
            return Vec::new();
        };
        let (name, path) = (next.name.clone(), pane.state.path.join(&next.name));
        if let Some(lister) = self.viewers.lister_mut(id) {
            let kept = (lister.wrap, lister.tab_width, lister.text_size);
            *lister = Lister::new(id, panel, path.clone());
            (lister.wrap, lister.tab_width, lister.text_size) = kept;
        }
        let cursor = Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name),
        };
        vec![
            Effect::Dispatch(panel, vec![cursor]),
            self.open_file(id, path),
        ]
    }

    /// Ctrl+Q from `panel`: Quick View of its cursor in the other panel, or off again.
    fn toggle_quick(&mut self, panel: PanelId) -> Vec<Effect> {
        if self.viewers.quick.take().is_some() {
            return Vec::new();
        }
        self.viewers.quick = Some(Quick::new(panel));
        self.quick_follow()
    }

    /// Quick View catches up with its panel's cursor: a new file opens, a directory is counted.
    fn quick_follow(&mut self) -> Vec<Effect> {
        let Some(source) = self.viewers.quick.as_ref().map(|quick| quick.source) else {
            return Vec::new();
        };
        let pane = self.pane(source);
        let entry = if pane.on_parent() {
            None
        } else {
            pane.entries().get(pane.cursor())
        };
        let wanted = quick::wanted(&pane.state.path, entry);
        let id = self.viewers.next_id();
        let Some(quick) = self.viewers.quick.as_mut() else {
            return Vec::new();
        };
        let Some((path, is_dir)) = wanted else {
            quick.shown = QuickShown::Nothing;
            return Vec::new();
        };
        if quick.path() == Some(&path) {
            return Vec::new();
        }
        if !is_dir {
            quick.show_file(Lister::new(id, source, path.clone()));
            return vec![self.open_file(id, path)];
        }
        quick.shown = QuickShown::Dir {
            path: path.clone(),
            summary: None,
        };
        let engine = self.engine.clone();
        let listing = Command::ListDir {
            panel: Some(source),
            path: Some(path.path.to_string_lossy().into_owned()),
        };
        let counting = async move {
            match engine.dispatch(listing).await {
                Ok(Outcome::Listing { entries, .. }) => Ok(DirSummary::of(&entries)),
                Ok(_) => Ok(DirSummary::default()),
                Err(error) => Err(error.code()),
            }
        };
        vec![Effect::Task(Task::perform(counting, move |summary| {
            wrap(ViewerMessage::Summary(path, summary))
        }))]
    }

    /// A key while the Quick View panel is the active one: the viewer's keys scroll and switch
    /// it; Escape, N, P and every other key stay the panels'.
    fn quick_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let quick = self.viewers.quick.as_ref()?;
        let QuickShown::File(lister) = &quick.shown else {
            return None;
        };
        if quick.target() != self.active || self.prompt.is_some() {
            return None;
        }
        let key = keys::action(chord)?;
        // With the text size keys off, Ctrl+NumAdd and Ctrl+NumSubtract are the panel's again.
        let given_back = key.sizes_text() && !self.viewers.text_zoom.enabled;
        if given_back
            || matches!(
                key,
                ListerKey::Close | ListerKey::NextFile | ListerKey::PreviousFile
            )
        {
            return None;
        }
        let id = lister.id;
        Some(self.lister_does(id, key))
    }
}

/// The bar open in the top Lister, for the tests.
#[cfg(test)]
fn bar_of(viewers: &Viewers) -> Option<find::BarKind> {
    viewers.open.last().and_then(|lister| lister.find.bar)
}

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;
