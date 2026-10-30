//! The widget: takes all the room it gets and fits the session's grid to it, draws the screen,
//! and turns keys and the mouse into input for the session. From iced_term 0.8's `view.rs`
//! (MIT).

use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::Point as GridPoint,
    selection::{Selection, SelectionType},
    term::TermMode,
};
use iced::{
    Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::Renderer as _,
    advanced::graphics::geometry::Renderer as _,
    keyboard::{self, Modifiers},
    mouse::Cursor,
    widget::container,
};
use iced_core::{
    clipboard::Kind as ClipboardKind,
    mouse::{self, Click},
    widget::operation::{self, Focusable},
};
use iced_graphics::core::{
    Widget,
    widget::{Tree, tree},
};
use rc_term::EventProxy;

use crate::{
    Event, Terminal, draw,
    input::{self, KeyInput},
    snapshot::Snapshot,
};

/// A [`Terminal`] on screen.
pub struct TerminalView<'a> {
    term: &'a Terminal,
}

impl<'a> TerminalView<'a> {
    /// The widget for `term`, filling its parent, on the terminal's background.
    pub fn show(term: &'a Terminal) -> Element<'a, Event> {
        container(Self { term })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| term.theme.container_style())
            .into()
    }

    /// Gives the keyboard to the terminal whose [`Terminal::widget_id`] is `id`.
    pub fn focus<Message: 'static>(id: iced::widget::Id) -> iced::Task<Message> {
        iced::widget::operation::focus(id)
    }

    /// Fits the session's grid to `size`; after a new size or font only.
    fn fit(&self, state: &mut ViewState, size: Size) {
        let cell = self.term.cell();
        if state.size == size && state.cell == cell {
            return;
        }
        state.size = size;
        state.cell = cell;
        let session = self.term.session();
        if let Some(grid) = input::grid_size(size, cell)
            && grid != session.size()
        {
            // An ended session keeps its last size; nothing is left to tell.
            let _ = session.resize(grid);
        }
        self.term.redraw();
    }

    /// A key; whether it was the terminal's.
    fn on_key(
        &self,
        state: &mut ViewState,
        event: &keyboard::Event,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
    ) -> bool {
        let (key, physical, modifiers, text) = match event {
            keyboard::Event::ModifiersChanged(modifiers) => {
                state.modifiers = *modifiers;
                return false;
            }
            keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                text,
                ..
            } => (key, *physical_key, *modifiers, text),
            _ => return false,
        };
        if self
            .term
            .reserved
            .iter()
            .any(|reserved| reserved.matches(key, physical, modifiers))
        {
            return false;
        }
        if self.term.copy.ctrl_c && input::is_ctrl_c(key, modifiers) && self.copy(clipboard, true) {
            return true;
        }
        // Without text on the clipboard Ctrl+V stays the program's: Claude Code pastes an image.
        if self.term.copy.ctrl_v && input::is_ctrl_v(key, modifiers) && self.paste(clipboard, shell)
        {
            return true;
        }
        let mode = *self.term.session().term().lock().mode();
        let bindings = &self.term.bindings;
        match input::key_input(bindings, key, modifiers, text.as_deref(), mode) {
            KeyInput::Write(bytes) => self.term.type_bytes(bytes),
            KeyInput::Copy => {
                self.copy(clipboard, false);
            }
            KeyInput::Paste => {
                self.paste(clipboard, shell);
            }
            KeyInput::PageUp => self.scroll(Scroll::PageUp),
            KeyInput::PageDown => self.scroll(Scroll::PageDown),
            KeyInput::None => return false,
        }
        true
    }

    /// The selection to the clipboard, dropped after when `clear`; whether there was one.
    fn copy(&self, clipboard: &mut dyn iced_graphics::core::Clipboard, clear: bool) -> bool {
        let shared = self.term.session().term();
        let selected = input::take_selection(&mut *shared.lock(), clear);
        let Some(text) = selected else {
            return false;
        };
        clipboard.write(ClipboardKind::Standard, text);
        true
    }

    /// The clipboard into the session; text with a line break or a control character goes to
    /// the app to confirm.
    /// Whether the clipboard held text.
    fn paste(
        &self,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
    ) -> bool {
        let Some(text) = clipboard.read(ClipboardKind::Standard) else {
            return false;
        };
        if Terminal::asks_before_pasting(&text) {
            shell.publish(Event::Paste(self.term.id(), text));
        } else {
            self.term.paste(&text);
        }
        true
    }

    fn scroll(&self, scroll: Scroll) {
        self.term.session().term().lock().scroll_display(scroll);
    }

    /// A mouse event at `position` (relative to the widget, `None` outside it); whether it was
    /// the terminal's.
    fn on_mouse(
        &self,
        state: &mut ViewState,
        event: &mouse::Event,
        position: Option<Point>,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
    ) -> bool {
        let Some(position) = position else {
            // A drag ends wherever the button comes up.
            if let mouse::Event::ButtonReleased(mouse::Button::Left) = event {
                state.dragging = false;
            }
            return false;
        };
        let cell = self.term.cell();
        let shared = self.term.session().term();
        let (point, mode) = {
            let term = shared.lock();
            let point = input::grid_point(
                position,
                cell,
                term.columns(),
                term.screen_lines(),
                term.grid().display_offset(),
            );
            (point, *term.mode())
        };
        state.grid_point = point;
        let reports = input::mouse_to_program(mode, state.modifiers, self.term.copy.shift_selects);
        match event {
            mouse::Event::CursorMoved { .. } => {
                if !state.dragging {
                    return false;
                }
                if state.reporting && mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
                {
                    self.report(input::LEFT_MOVE, state, true, mode);
                } else if let Some(selection) = shared.lock().selection.as_mut() {
                    selection.update(point, input::side(position.x, cell));
                }
                true
            }
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                if !state.focus {
                    return false;
                }
                state.dragging = true;
                state.reporting = reports;
                if reports {
                    self.report(input::LEFT_BUTTON, state, true, mode);
                } else {
                    let click = Click::new(position, mouse::Button::Left, state.last_click);
                    let kind = match click.kind() {
                        mouse::click::Kind::Single => SelectionType::Simple,
                        mouse::click::Kind::Double => SelectionType::Semantic,
                        mouse::click::Kind::Triple => SelectionType::Lines,
                    };
                    state.last_click = Some(click);
                    let side = input::side(position.x, cell);
                    shared.lock().selection = Some(Selection::new(kind, point, side));
                }
                true
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                let dragged = std::mem::replace(&mut state.dragging, false);
                if dragged && state.reporting {
                    self.report(input::LEFT_BUTTON, state, false, mode);
                } else if dragged && self.term.copy.on_select {
                    self.copy(clipboard, false);
                }
                dragged
            }
            // Windows Terminal copies a selection on a right click and pastes without one.
            mouse::Event::ButtonPressed(mouse::Button::Right) if !reports => {
                if self.term.copy.right_click && self.copy(clipboard, true) {
                    true
                } else if cfg!(windows) {
                    self.paste(clipboard, shell);
                    true
                } else {
                    false
                }
            }
            mouse::Event::WheelScrolled { delta } => {
                let lines = input::wheel_lines(*delta, &mut state.scroll_pixels, cell.height);
                if lines == 0 {
                    return false;
                }
                if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
                    self.term.send(input::alternate_scroll(lines));
                } else {
                    self.scroll(Scroll::Delta(lines));
                }
                true
            }
            _ => false,
        }
    }

    /// Tells a program that asked for the mouse what the left button did at the cell under it.
    fn report(&self, button: u8, state: &ViewState, pressed: bool, mode: TermMode) {
        let report = input::mouse_report(button, state.modifiers, state.grid_point, pressed, mode);
        if let Some(bytes) = report {
            self.term.send(bytes);
        }
    }
}

impl Widget<Event, Theme, iced::Renderer> for TerminalView<'_> {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ViewState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ViewState::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        iced::advanced::layout::Node::new(size)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: iced_core::Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree.state.downcast_mut::<ViewState>();
        operation.focusable(Some(self.term.widget_id()), layout.bounds(), state);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<ViewState>();
        let bounds = layout.bounds();
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        // Drawn at the frame's origin and moved into place: a clipping parent keeps it.
        let geometry = self.term.cache.draw(renderer, bounds.size(), |frame| {
            // Copied under the lock, drawn without it: the session's pump waits for the copy
            // only, never for the frame.
            let snapshot = {
                let shared = self.term.session().term();
                let term = shared.lock();
                Snapshot::take::<EventProxy>(&self.term.theme, &term)
            };
            draw::screen(frame, self.term, &snapshot, state.focus);
        });
        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            renderer.draw_geometry(geometry);
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: iced_graphics::core::Layout<'_>,
        cursor: Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<ViewState>();
        let bounds = layout.bounds();
        self.fit(state, bounds.size());
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event {
            state.focus = cursor.is_over(bounds);
        }
        // Shift selects on a click that also gives the focus.
        if let iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }
        let handled = match event {
            iced::Event::Keyboard(event) if state.focus => {
                self.on_key(state, event, clipboard, shell)
            }
            iced::Event::Mouse(event) => {
                let position = cursor.position_in(bounds);
                self.on_mouse(state, event, position, clipboard, shell)
            }
            _ => false,
        };
        if handled {
            shell.capture_event();
        }
        if handled || state.focus != state.drawn_focus {
            state.drawn_focus = state.focus;
            self.term.redraw();
            shell.request_redraw();
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: iced_core::Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let mode = *self.term.session().term().lock().mode();
        if cursor.is_over(layout.bounds()) && !mode.intersects(TermMode::MOUSE_MODE) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::Idle
        }
    }
}

impl<'a> From<TerminalView<'a>> for Element<'a, Event, Theme, iced::Renderer> {
    fn from(widget: TerminalView<'a>) -> Self {
        Self::new(widget)
    }
}

/// What the widget remembers between events.
#[derive(Debug, Clone)]
struct ViewState {
    focus: bool,
    /// The focus the cached screen was drawn with (the cursor is hollow without it).
    drawn_focus: bool,
    dragging: bool,
    /// The drag goes to the program, not into a selection.
    reporting: bool,
    last_click: Option<Click>,
    /// Wheel pixels short of a line.
    scroll_pixels: f32,
    modifiers: Modifiers,
    /// The size and cell the grid was last fitted to.
    size: Size,
    cell: Size,
    /// The cell under the mouse.
    grid_point: GridPoint,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            focus: false,
            drawn_focus: false,
            dragging: false,
            reporting: false,
            last_click: None,
            scroll_pixels: 0.0,
            modifiers: Modifiers::empty(),
            size: Size::new(0.0, 0.0),
            cell: Size::new(0.0, 0.0),
            grid_point: GridPoint::default(),
        }
    }
}

impl Focusable for ViewState {
    fn is_focused(&self) -> bool {
        self.focus
    }

    fn focus(&mut self) {
        self.focus = true;
    }

    fn unfocus(&mut self) {
        self.focus = false;
    }
}
