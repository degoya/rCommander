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
//! - `lister.rs` - one file's view: mode, encoding, place, page; `find.rs` - its search bar;
//!   `copy.rs` - Ctrl+A and Ctrl+C over its whole text (RC-110-06).
//! - `load.rs` - the reads behind it; `picture.rs` - decoding a picture within limits.
//! - `quick.rs` - Quick View's state, `quick_follow.rs` - it following the cursor once that
//!   rested (RC-110-28); `draw.rs` - drawing both; `window.rs` - Lister's window.
//! - `markdown.rs` - a Markdown file's preview, `markdown_view.rs` drawing it, `links.rs` its
//!   links (RC-020-09), `code_lexer.rs` the colours of its code blocks (RC-110-06); `text_size.rs` - Ctrl+wheel, Ctrl++, Ctrl+- and Ctrl+0 (RC-020-12).
//! - `document.rs` - a plugin's document with several views (RC-100-10): `spans.rs` its coloured
//!   text, `tree.rs` its tree, `views.rs` switching them, `tree_view.rs` drawing the tree and the
//!   list of views; `run.rs` - a lister's requests as tasks; `plugin.rs` - a file opened with its
//!   viewer plugin, or built in (RC-100-13); `walk.rs` - N and P, through the marked files or
//!   none (RC-100-91); `credentials.rs` - a credential file (`.env`, `.npmrc`, ...), which no
//!   plugin may see, coloured in process by the bundled viewers' crates (RC-110-07);
//!   `cert.rs` - a certificate or key file as its report and tree, put into words by
//!   `cert_words.rs` (RC-110-08).

mod cert;
mod cert_words;
mod code_lexer;
mod copy;
mod credentials;
pub(in crate::app) mod document;
mod draw;
mod exif;
mod find;
mod keys;
mod last_page;
pub(in crate::app) mod links;
mod lister;
mod load;
mod markdown;
mod markdown_view;
mod output;
pub(in crate::app) mod picture;
mod plugin;
mod quick;
mod quick_follow;
mod run;
mod select;
mod selectable;
mod spans;
mod text_size;
mod tree;
mod tree_view;
mod views;
mod walk;
mod window;

use iced::{
    Size,
    widget::{Id, operation, text_editor::Action},
};
use rc_engine::{
    Chord, Command, EngineEvent, ListerFiles, PanelId, VfsPath, settings::VIEWER_TOKEN_COLORS_KEY,
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
    quick_follow::QuickPause,
    text_size::{TextSizeMsg, TextZoom},
    views::TokenColors,
    walk::Walk,
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
    /// `viewer.quick_view_pause_ms` (RC-110-28).
    pub quick_pause: QuickPause,
    /// The viewer plugins, loaded when first needed (RC-100-13).
    plugins: Plugins,
    /// The files N and P step through, by Lister; a Lister not here steps through the panel's.
    walks: Vec<(u64, Walk)>,
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

    /// The files the Lister `id` steps through.
    fn walk(&self, id: u64) -> Walk {
        self.walks
            .iter()
            .find(|(lister, _)| *lister == id)
            .map_or(Walk::Panel, |(_, walk)| walk.clone())
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
    /// Ctrl+A or Ctrl+C the focused text editor hands to its lister (`copy.rs`).
    Key(u64, ListerKey),
    /// The whole text read for Ctrl+C, or why not.
    Copied(u64, Result<String, &'static str>),
    Viewport(u64, Size),
    Scrollbar(u64, f32),
    BarInput(u64, String),
    BarSubmit(u64),
    Toggle(u64, FindOption),
    Summary(VfsPath, Result<DirSummary, &'static str>),
    /// The pause after the cursor's move of a turn ended: Quick View shows what is under it.
    QuickRested(u64),
    /// `viewer.quick_view_pause_ms` read.
    QuickPause(QuickPause),
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
            Message::Engine(EngineEvent::ViewFile { panel, path, files }) => {
                Some(self.open_lister(*panel, path.clone(), *files))
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
            Message::Engine(EngineEvent::SettingChanged { key }) if quick_follow::concerns(key) => {
                Some(vec![Effect::Task(self.quick_pause_task())])
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
            ViewerMessage::QuickRested(turn) => return self.quick_rested(*turn),
            ViewerMessage::QuickPause(pause) => {
                self.viewers.quick_pause = *pause;
                return Vec::new();
            }
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
            | ViewerMessage::Key(id, _)
            | ViewerMessage::Copied(id, _)
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
            ViewerMessage::Key(_, key) => return self.lister_does(id, key),
            ViewerMessage::Copied(_, copied) => return lister.copied(copied),
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
            | ViewerMessage::QuickRested(_)
            | ViewerMessage::QuickPause(_)
            | ViewerMessage::TextSize(_)
            | ViewerMessage::TokenColors(_)
            | ViewerMessage::Plugin(..) => Vec::new(),
        };
        self.run(id, requests)
    }

    /// F3: a new Lister on top, stepping through `files` (`walk.rs`); the walks of Listers
    /// closed meanwhile go.
    fn open_lister(&mut self, panel: PanelId, path: VfsPath, files: ListerFiles) -> Vec<Effect> {
        let id = self.viewers.next_id();
        let marked = self.pane(panel).chosen_names();
        let viewers = &mut self.viewers;
        let open: Vec<u64> = viewers.open.iter().map(|lister| lister.id).collect();
        viewers.walks.retain(|(lister, _)| open.contains(lister));
        viewers.walks.push((id, Walk::of(files, marked)));
        viewers.open.push(Lister::new(id, panel, path.clone()));
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
        // With the text size keys off, Ctrl+NumAdd and Ctrl+NumSubtract are the panel's again;
        // Ctrl+A and Ctrl+C are, unless a preview selects.
        let given_back = (key.sizes_text() && !self.viewers.text_zoom.enabled)
            || (matches!(key, ListerKey::SelectAll | ListerKey::Copy)
                && lister.selection().is_none());
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
