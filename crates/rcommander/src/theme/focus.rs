//! The focus ring of a dialog (RC-080-16): every control of a dialog is a stop that Tab and
//! Shift+Tab reach, as in a native Windows dialog. iced 0.14 gives the keyboard to text fields
//! only; [`Stop`] wraps any other control - a button, a checkbox, a pick list, a group of radio
//! buttons, a list or a tree - and makes it one more stop: it reports itself to the operations
//! as focusable, so `app/field_walk.rs` walks it among the text fields in reading order; it draws
//! the focus ring round its control while it has the keyboard; and it takes the keys a native
//! control takes then:
//!
//! - **Enter** presses a focused button ([`button`]); on any other stop it is the dialog's, which
//!   takes its default button. A multi-line field keeps its own Enter (it captures it).
//! - **Space** presses a focused button and toggles a focused checkbox or radio button
//!   ([`toggle`]).
//! - **The arrows** choose the option before (Up, Left) or after (Down, Right) in a focused pick
//!   list or radio group, and move the cursor of a focused list ([`choice`]); a stop without
//!   arrow messages leaves them to the dialog.
//!
//! A key a stop takes is captured, so the dialog sees it as `Message::KeyCaptured` and does not
//! act on it a second time. A click anywhere takes the ring away (a text field clicked into has
//! the keyboard then, as in Windows). A disabled control is no stop, as in Windows. The ring is
//! the control's bounds grown by [`RING_GAP`], [`RING_WIDTH`] wide in the text colour - WinUI's
//! focus visual (`docs/design.md`, States). A stop keeps its focus while the view is rebuilt and
//! loses it when the control at its place changes (another label), so a new dialog never opens
//! with a stale ring. A stop may carry an access key ([`Stop::letter`]): the window's
//! `theme::mnemonic::Scope` arms it, and it presses itself on that key; a text field takes the
//! access key of its label through [`Stop::field`], which is no stop of its own.

mod state;

use std::{
    any::Any,
    hash::{DefaultHasher, Hash, Hasher},
};

use iced::{
    Color, Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Id, Operation, Tree, tree},
    },
    event::Event,
    keyboard::{self, Key, key::Named},
    touch,
};

pub use state::{State, ring};

/// The space between a control's edge and its focus ring.
pub const RING_GAP: f32 = 2.0;
/// The width of the focus ring.
pub const RING_WIDTH: f32 = 2.0;
/// The corner radius of the ring: a button's 4 grown by the gap.
const RING_RADIUS: f32 = 6.0;

/// What a stop is, for the keys it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Enter and Space press it.
    Button,
    /// Space toggles it.
    Toggle,
    /// The arrows choose before or after.
    Choice,
    /// A text field's access key ([`Stop::field`]): it gives the field the keyboard.
    Field,
}

/// One stop of the ring: `content` and what its keys send while it has the keyboard.
pub struct Stop<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    kind: Kind,
    /// The control's identity at its place: a hash of its label.
    key: u64,
    /// Enter (a button) or Space (a button, a toggle).
    on_press: Option<Message>,
    /// ArrowUp and ArrowLeft.
    on_previous: Option<Message>,
    /// ArrowDown and ArrowRight.
    on_next: Option<Message>,
    /// Whether the control takes the keyboard at all: a disabled one is no stop.
    enabled: bool,
    /// The id that `iced::widget::operation::focus` gives the ring to.
    id: Option<Id>,
    /// The access key, in lower case.
    letter: Option<char>,
}

/// A button as a stop: Enter and Space send `on_press`; `None` (disabled) makes it no stop.
/// `label` tells the button from the one at its place in another dialog.
pub fn button<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    label: &str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    Stop::button(content, label, on_press).into()
}

/// A checkbox or a radio button as a stop: Space sends `on_toggle` - the toggled value, or the
/// radio button's choice; `None` (disabled) makes it no stop.
pub fn toggle<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    label: &str,
    on_toggle: Option<Message>,
) -> Element<'a, Message> {
    Stop::toggle(content, label, on_toggle).into()
}

/// A pick list, a group of radio buttons, a list or a tree as one stop: the arrows send
/// `previous` and `next` (`None` at the end, or where the dialog moves the list's cursor itself).
pub fn choice<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    label: &str,
    previous: Option<Message>,
    next: Option<Message>,
) -> Element<'a, Message> {
    Stop::choice(content, label, previous, next).into()
}

/// A pick list over `options` with `selected` chosen as a stop: the arrows choose the option
/// before and after it through `on_select`, as a closed combo box does in Windows.
pub fn pick<'a, T, Message>(
    content: impl Into<Element<'a, Message>>,
    label: &str,
    options: &[T],
    selected: Option<&T>,
    on_select: impl Fn(T) -> Message,
) -> Element<'a, Message>
where
    T: PartialEq + Clone,
    Message: Clone + 'a,
{
    Stop::pick(content, label, moves(options, selected, &on_select)).into()
}

/// The messages the arrows send at a pick list: the option before and after the chosen one.
pub type Moves<Message> = (Option<Message>, Option<Message>);

/// What the arrows choose at a pick list over `options` with `selected` chosen: the messages
/// for the option before and after it, for [`Stop::pick`]. Taken before the options move into
/// their pick list, so a view clones no list for its stop (RC-080-53).
pub fn moves<T, Message>(
    options: &[T],
    selected: Option<&T>,
    on_select: &impl Fn(T) -> Message,
) -> Moves<Message>
where
    T: PartialEq + Clone,
{
    let (previous, next) = neighbours(options, selected);
    (
        previous.cloned().map(on_select),
        next.cloned().map(on_select),
    )
}

/// The options before and after `selected` in `options`; with none selected, the first after.
pub fn neighbours<'o, T: PartialEq>(
    options: &'o [T],
    selected: Option<&T>,
) -> (Option<&'o T>, Option<&'o T>) {
    let at = selected.and_then(|chosen| options.iter().position(|option| option == chosen));
    match at {
        Some(at) => (
            at.checked_sub(1).and_then(|before| options.get(before)),
            options.get(at + 1),
        ),
        None => (None, options.first()),
    }
}

impl<'a, Message: Clone + 'a> Stop<'a, Message> {
    /// [`button`] as a stop to give an id ([`Stop::id`]).
    pub fn button(
        content: impl Into<Element<'a, Message>>,
        label: &str,
        on_press: Option<Message>,
    ) -> Self {
        Self::new(content, Kind::Button, label, on_press, None, None)
    }

    /// [`toggle`] as a stop to give an id.
    pub fn toggle(
        content: impl Into<Element<'a, Message>>,
        label: &str,
        on_toggle: Option<Message>,
    ) -> Self {
        Self::new(content, Kind::Toggle, label, on_toggle, None, None)
    }

    /// [`choice`] as a stop to give an id.
    pub fn choice(
        content: impl Into<Element<'a, Message>>,
        label: &str,
        previous: Option<Message>,
        next: Option<Message>,
    ) -> Self {
        let mut stop = Self::new(content, Kind::Choice, label, None, previous, next);
        stop.enabled = true;
        stop
    }

    /// [`pick`] as a stop to give an id or an access key, the arrows' choices taken with
    /// [`moves`] - before the options moved into the pick list.
    pub fn pick(
        content: impl Into<Element<'a, Message>>,
        label: &str,
        (previous, next): Moves<Message>,
    ) -> Self {
        Self::choice(content, label, previous, next)
    }

    /// The one text field `content` as the target of its label's access key ([`Stop::letter`],
    /// given by `theme::layout::caption`): no stop of its own, as the field is one already, and
    /// no ring - the key gives the field the keyboard and types nothing.
    pub fn field(content: impl Into<Element<'a, Message>>, label: &str) -> Self {
        let mut stop = Self::new(content, Kind::Field, label, None, None, None);
        stop.enabled = true;
        stop
    }

    /// The stop with the id `id`: `iced::widget::operation::focus` gives it the ring - the safe
    /// answer of a question as it opens, as Windows does - and a dialog that follows the ring
    /// (`app/field_walk.rs`, `focused`) knows it by it.
    #[must_use]
    pub fn id(mut self, id: &'static str) -> Self {
        // The id names the control, whatever its label says now (RC-080-53).
        self.key = identity(true, id);
        self.id = Some(Id::new(id));
        self
    }

    /// The stop with the access key `letter` (`theme::mnemonic`).
    #[must_use]
    pub fn letter(mut self, letter: Option<char>) -> Self {
        self.letter = letter;
        self
    }
}

impl<'a, Message, Renderer> Stop<'a, Message, Renderer> {
    fn new(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        kind: Kind,
        label: &str,
        on_press: Option<Message>,
        on_previous: Option<Message>,
        on_next: Option<Message>,
    ) -> Self {
        Self {
            content: content.into(),
            kind,
            key: identity(false, label),
            enabled: on_press.is_some(),
            on_press,
            on_previous,
            on_next,
            id: None,
            letter: None,
        }
    }
}

/// Which control a stop is, across frames: its id when it has one, else its label - another
/// control at the same place takes the ring away.
fn identity(by_id: bool, name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    (by_id, name).hash(&mut hasher);
    hasher.finish()
}

impl<Message: Clone, Renderer> Stop<'_, Message, Renderer> {
    /// What `key` sends while the stop has the keyboard; `None` leaves the key to the dialog.
    fn action(&self, key: &Key, modifiers: keyboard::Modifiers) -> Option<Message> {
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return None;
        }
        match (key, self.kind) {
            (Key::Named(Named::Enter), Kind::Button)
            | (Key::Named(Named::Space), Kind::Button | Kind::Toggle) => self.on_press.clone(),
            (Key::Named(Named::ArrowUp | Named::ArrowLeft), Kind::Choice) => {
                self.on_previous.clone()
            }
            (Key::Named(Named::ArrowDown | Named::ArrowRight), Kind::Choice) => {
                self.on_next.clone()
            }
            _ => None,
        }
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Stop<'_, Message, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.key, self.kind, self.letter))
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        // Another control at this place, or this one disabled: the ring goes.
        if state.key != self.key || !self.enabled {
            state.focused = false;
        }
        state.key = self.key;
        state.kind = self.kind;
        state.letter = self.letter;
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.kind == Kind::Field {
            // The field reports itself first: the access key's place is the field's.
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
            let state = tree.state.downcast_mut::<State>();
            operation.custom(self.id.as_ref(), layout.bounds(), state as &mut dyn Any);
            return;
        }
        if self.enabled {
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(self.id.as_ref(), layout.bounds(), &mut *state);
            operation.custom(self.id.as_ref(), layout.bounds(), state as &mut dyn Any);
        }
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Armed by the scope for its access key: pressed, the key goes no further.
        if let Event::Keyboard(keyboard::Event::KeyPressed { .. }) = event {
            let state = tree.state.downcast_mut::<State>();
            if std::mem::take(&mut state.armed) {
                if let Some(message) = self.on_press.clone() {
                    shell.publish(message);
                }
                shell.capture_event();
                shell.request_redraw();
                return;
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if state.focused {
                    state.focused = false;
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused && !shell.is_event_captured() =>
            {
                if let Some(message) = self.action(key, *modifiers) {
                    shell.publish(message);
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if tree.state.downcast_ref::<State>().focused {
            renderer.fill_quad(
                ring(layout.bounds(), theme.palette().text),
                Color::TRANSPARENT,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Renderer> From<Stop<'a, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(stop: Stop<'a, Message, Renderer>) -> Self {
        Element::new(stop)
    }
}

#[cfg(test)]
#[path = "focus_tests.rs"]
mod tests;
