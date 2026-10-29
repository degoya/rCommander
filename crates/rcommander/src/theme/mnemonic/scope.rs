//! The widget round a window's content that presses the access keys of its dialogs
//! (RC-080-16). A key press that is an access key ([`super::pressed`]) is looked up among the
//! stops of the top dialog card - the card marked `theme::DIALOG_ID` last in the window's tree,
//! as Tab walks it - before any control sees it:
//!
//! - one stop with the letter: it gets the focus ring and is armed, the key goes on without its
//!   text (so no text field types the letter), and the armed stop presses itself;
//! - several: the ring goes to the next of them, nothing is pressed - as Windows does with a
//!   letter two controls share;
//! - none, or the letter alone while no button or checkbox of that card has the ring: the key
//!   goes on as it came. A dialog beneath the top card never takes a letter.
//!
//! A key the scope took is captured: the app gets it as `Message::KeyCaptured`. Without a card
//! in the window nothing changes for any key.

use std::any::Any;

use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Id, Operation, Tree, operation::Focusable},
    },
    event::Event,
    keyboard,
};

use crate::theme::{
    focus::{self, Kind},
    layout::Cards,
};

/// A window's content whose top dialog card presses its access keys.
pub struct Scope<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    paused: bool,
}

impl<'a, Message, Renderer> Scope<'a, Message, Renderer> {
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            paused: false,
        }
    }

    /// Whether the scope lets every key through untouched - while a shortcut is recorded, the
    /// letter is the recording's, never a button's. The scope stays in the tree either way, so
    /// the content keeps its widget state.
    #[must_use]
    pub fn paused(mut self, paused: bool) -> Self {
        self.paused = paused;
        self
    }
}

/// The stop a key reaches: its card, its place among the card's focusables, and whether it is
/// pressed or only gets the ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Target {
    card: usize,
    at: usize,
    press: bool,
}

/// What one card holds for the key: its focusables, the stops with the letter, and the
/// focusable with the keyboard - with whether it is a button or a checkbox.
#[derive(Debug, Default)]
struct Seen {
    count: usize,
    hits: Vec<usize>,
    focused: Option<(usize, bool)>,
}

/// The first pass: the stops of each card with `letter`.
struct Find {
    letter: char,
    alt: bool,
    cards: Cards,
    /// Card `n` at `n - 1`.
    seen: Vec<Seen>,
}

impl Find {
    fn new(letter: char, alt: bool) -> Self {
        Self {
            letter,
            alt,
            cards: Cards::default(),
            seen: Vec::new(),
        }
    }

    fn here(&mut self) -> Option<&mut Seen> {
        let card = self.cards.inside?;
        self.seen.get_mut(card - 1)
    }

    /// The stop of the top card the key reaches, if any.
    fn target(&self) -> Option<Target> {
        let card = self.seen.len();
        let seen = self.seen.last()?;
        // The letter alone only while a button or a checkbox has the ring: a field, a list or a
        // pick list takes letters itself, and a dialog that just opened takes none by surprise.
        if !self.alt && !seen.focused.is_some_and(|(_, presses)| presses) {
            return None;
        }
        match seen.hits.as_slice() {
            [] => None,
            [only] => Some(Target {
                card,
                at: *only,
                press: true,
            }),
            many => {
                let from = seen.focused.map(|(at, _)| at);
                let next = many
                    .iter()
                    .copied()
                    .find(|&hit| from.is_some_and(|from| hit > from))
                    .unwrap_or(many[0]);
                Some(Target {
                    card,
                    at: next,
                    press: false,
                })
            }
        }
    }
}

impl Operation for Find {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let (outer, card) = self.cards.enter();
        if card {
            self.seen.push(Seen::default());
        }
        operate(self);
        self.cards.inside = outer;
    }

    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.cards.container(id);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        let Some(seen) = self.here() else {
            return;
        };
        if state.is_focused() {
            // A text field until its stop says otherwise (a stop reports itself next).
            seen.focused = Some((seen.count, false));
        }
        seen.count += 1;
    }

    fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
        let letter = self.letter;
        let Some(stop) = state.downcast_ref::<focus::State>() else {
            return;
        };
        let Some(seen) = self.here() else {
            return;
        };
        // The stop's own focusable was the last one counted.
        let at = seen.count.saturating_sub(1);
        if stop.letter() == Some(letter) {
            seen.hits.push(at);
        }
        if stop.is_focused() && matches!(stop.kind(), Kind::Button | Kind::Toggle) {
            seen.focused = Some((at, true));
        }
    }
}

/// The second pass: the ring to the target, away from every other focusable, and the target
/// armed when it is pressed.
struct Arm {
    cards: Cards,
    target: Target,
    seen: usize,
}

impl Arm {
    fn new(target: Target) -> Self {
        Self {
            cards: Cards::default(),
            target,
            seen: 0,
        }
    }
}

impl Operation for Arm {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let (outer, _) = self.cards.enter();
        operate(self);
        self.cards.inside = outer;
    }

    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.cards.container(id);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.cards.inside == Some(self.target.card) {
            if self.seen == self.target.at {
                state.focus();
            } else {
                state.unfocus();
            }
            self.seen += 1;
        } else if state.is_focused() {
            state.unfocus();
        }
    }

    fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
        let mine = self.cards.inside == Some(self.target.card) && self.seen == self.target.at + 1;
        if let Some(stop) = state.downcast_mut::<focus::State>() {
            stop.arm(mine && self.target.press);
        }
    }
}

/// After the key: no stop stays armed, also one that did not see it.
struct Disarm;

impl Operation for Disarm {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
        if let Some(stop) = state.downcast_mut::<focus::State>() {
            stop.arm(false);
        }
    }
}

/// `event` without the text it types.
fn untyped(event: &Event) -> Event {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location,
            modifiers,
            repeat,
            ..
        }) => Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: modified_key.clone(),
            physical_key: *physical_key,
            location: *location,
            modifiers: *modifiers,
            text: None,
            repeat: *repeat,
        }),
        other => other.clone(),
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Scope<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
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
        let content = &mut tree.children[0];
        let mut target = None;
        if !self.paused
            && let Some((letter, alt)) = super::pressed(event)
            && !shell.is_event_captured()
        {
            let mut find = Find::new(letter, alt);
            self.content
                .as_widget_mut()
                .operate(content, layout, renderer, &mut find);
            target = find.target();
        }
        let Some(target) = target else {
            self.content.as_widget_mut().update(
                content, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
            return;
        };
        self.content
            .as_widget_mut()
            .operate(content, layout, renderer, &mut Arm::new(target));
        // Captured first: the armed stop takes it all the same, and no other control does.
        shell.capture_event();
        shell.request_redraw();
        if target.press {
            self.content.as_widget_mut().update(
                content,
                &untyped(event),
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            self.content
                .as_widget_mut()
                .operate(content, layout, renderer, &mut Disarm);
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

impl<'a, Message, Renderer> From<Scope<'a, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(scope: Scope<'a, Message, Renderer>) -> Self {
        Element::new(scope)
    }
}
