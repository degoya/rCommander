//! A terminal as the app keeps it: the session it shows and how it draws it (font, colours,
//! key bindings, the drawn frame).

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use alacritty_terminal::{event::Event as TermUiEvent, grid::Scroll, term::ClipboardType};
use iced::{
    Font, Size, Subscription,
    futures::{Stream, StreamExt, stream},
    keyboard::{Key, Modifiers},
    widget::canvas::Cache,
};
use rc_term::{TermEvent, TermId, TermSession};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

use crate::{
    ColorPalette,
    bindings::BindingsLayout,
    font::TermFont,
    input::{acts_beyond_typing, paste_bytes},
    theme::Theme,
};

/// What the widget tells the app.
#[derive(Debug, Clone)]
pub enum Event {
    /// The session's screen or title changed: draw it again ([`Terminal::redraw`]).
    Redraw(TermId),
    /// Text to paste that holds a line break, which would run commands, or another control
    /// character but a tab, which would act as its key: the app asks the person first, then
    /// calls [`Terminal::paste`].
    Paste(TermId, String),
    /// A program put text on the clipboard (OSC 52); the app writes it when
    /// [`CopySettings::osc52`] allows.
    Copy(TermId, String),
}

/// A chord the app keeps for itself: the widget neither sends it to the program nor captures
/// it. A character key, matched by the character the layout gives it without modifiers (lower
/// case), as the app's keymap reads keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedKey {
    pub character: String,
    pub modifiers: Modifiers,
}

impl ReservedKey {
    /// Whether `key` pressed with `modifiers` is this chord.
    #[must_use]
    pub fn matches(&self, key: &Key, modifiers: Modifiers) -> bool {
        matches!(key, Key::Character(character) if character.to_lowercase() == self.character)
            && modifiers == self.modifiers
    }
}

#[derive(Debug, Clone)]
pub struct FontSettings {
    pub size: f32,
    /// The line height, relative to the size.
    pub scale_factor: f32,
    pub font_type: Font,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            size: 14.0,
            scale_factor: 1.3,
            font_type: Font::MONOSPACE,
        }
    }
}

/// How the widget copies its selection and pastes besides Ctrl+Shift+C and Ctrl+Shift+V, as
/// Windows Terminal does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopySettings {
    /// Ctrl+C copies a selection and clears it; without one it goes to the program.
    pub ctrl_c: bool,
    /// A right click copies a selection and clears it; without one it pastes on Windows.
    pub right_click: bool,
    /// Letting go of the mouse after selecting copies the selection.
    pub on_select: bool,
    /// Ctrl+V pastes the clipboard's text; without text it goes to the program.
    pub ctrl_v: bool,
    /// A program's OSC 52 copy reaches the clipboard ([`Event::Copy`]; the app honours it).
    pub osc52: bool,
    /// Shift with the mouse selects even while the program takes the mouse.
    pub shift_selects: bool,
}

impl Default for CopySettings {
    fn default() -> Self {
        Self {
            ctrl_c: true,
            right_click: true,
            on_select: false,
            ctrl_v: true,
            osc52: true,
            shift_selects: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub font: FontSettings,
    pub palette: ColorPalette,
    pub reserved: Vec<ReservedKey>,
    pub copy: CopySettings,
}

/// One session in a widget. Keys, pastes and mouse reports go to the session; its output comes
/// back through [`Terminal::subscription`].
pub struct Terminal {
    session: Arc<TermSession>,
    widget_id: iced::widget::Id,
    pub(crate) font: TermFont,
    pub(crate) theme: Theme,
    pub(crate) bindings: BindingsLayout,
    pub(crate) reserved: Vec<ReservedKey>,
    pub(crate) copy: CopySettings,
    /// The drawn screen, until the session or the look changes.
    pub(crate) cache: Cache,
}

impl Terminal {
    #[must_use]
    pub fn new(session: Arc<TermSession>, settings: &Settings) -> Self {
        Self {
            session,
            widget_id: iced::widget::Id::unique(),
            font: TermFont::new(&settings.font),
            theme: Theme::new(&settings.palette),
            bindings: BindingsLayout::default(),
            reserved: settings.reserved.clone(),
            copy: settings.copy,
            cache: Cache::default(),
        }
    }

    #[must_use]
    pub fn id(&self) -> TermId {
        self.session.id()
    }

    #[must_use]
    pub fn session(&self) -> &Arc<TermSession> {
        &self.session
    }

    /// The id the app focuses ([`crate::TerminalView::focus`]).
    #[must_use]
    pub fn widget_id(&self) -> &iced::widget::Id {
        &self.widget_id
    }

    /// New colours, for a light or dark look.
    pub fn set_palette(&mut self, palette: &ColorPalette) {
        self.theme = Theme::new(palette);
        self.redraw();
    }

    /// A new font; the widget fits the grid to it on its next event.
    pub fn set_font(&mut self, font: &FontSettings) {
        self.font = TermFont::new(font);
        self.redraw();
    }

    /// New ways to copy, from the app's settings.
    pub fn set_copy(&mut self, copy: CopySettings) {
        self.copy = copy;
    }

    /// Draws the screen again on the next frame.
    pub fn redraw(&self) {
        self.cache.clear();
    }

    /// Pastes `text` as typed input - bracketed when the program asked for it - and shows the
    /// end of the screen. Text with a line break or a control character should have been
    /// confirmed ([`Event::Paste`]).
    pub fn paste(&self, text: &str) {
        let mode = *self.session.term().lock().mode();
        self.type_bytes(paste_bytes(text, mode));
    }

    /// Whether pasting `text` should be confirmed first: a line break, or another control
    /// character but a tab.
    #[must_use]
    pub fn asks_before_pasting(text: &str) -> bool {
        acts_beyond_typing(text)
    }

    /// Bytes for the program, then the view back at the prompt, as typing does.
    pub(crate) fn type_bytes(&self, bytes: Vec<u8>) {
        self.send(bytes);
        self.session.term().lock().scroll_display(Scroll::Bottom);
        self.redraw();
    }

    /// Bytes for the program as they are (a mouse report, a scroll by cursor keys). A session
    /// whose program has ended takes nothing; there is no one left to tell.
    pub(crate) fn send(&self, bytes: Vec<u8>) {
        let _ = self.session.write(bytes);
    }

    /// One cell, in whole pixels.
    pub(crate) fn cell(&self) -> Size<f32> {
        self.font.cell()
    }

    /// A [`Event::Redraw`] whenever the session changed, until it ends.
    pub fn subscription(&self) -> Subscription<Event> {
        Subscription::run_with(Watch(Arc::clone(&self.session)), changes)
    }
}

/// The session a subscription watches; one subscription per session.
struct Watch(Arc<TermSession>);

impl Hash for Watch {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rc-term-widget.session".hash(state);
        self.0.id().hash(state);
    }
}

/// A redraw per burst of changes: what queued while the app drew is one redraw, not many; a
/// program's copies (OSC 52) come before it, each on its own. The stream ends with the session -
/// iced_term panicked when its channel closed.
fn changes(watch: &Watch) -> impl Stream<Item = Event> + use<> {
    let id = watch.0.id();
    stream::unfold(Some(watch.0.subscribe()), move |receiver| async move {
        let mut receiver = receiver?;
        let mut events = Vec::new();
        let mut ended = match receiver.recv().await {
            Ok(event) => burst(id, event, &mut events),
            Err(RecvError::Closed) => true,
            Err(RecvError::Lagged(_)) => false,
        };
        while !ended {
            match receiver.try_recv() {
                Ok(event) => ended = burst(id, event, &mut events),
                Err(TryRecvError::Closed) => ended = true,
                Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Empty) => break,
            }
        }
        events.push(Event::Redraw(id));
        Some((events, (!ended).then_some(receiver)))
    })
    .flat_map(stream::iter)
}

/// Takes one session event of a burst into `events`; whether the session ended.
fn burst(id: TermId, event: TermEvent, events: &mut Vec<Event>) -> bool {
    match event {
        TermEvent::Exited(_) => return true,
        // Only the clipboard: Windows has no primary selection to fill.
        TermEvent::Ui(TermUiEvent::ClipboardStore(ClipboardType::Clipboard, text)) => {
            events.push(Event::Copy(id, text));
        }
        _ => {}
    }
    false
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
