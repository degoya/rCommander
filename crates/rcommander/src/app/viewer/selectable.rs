//! One block of the Markdown preview that the mouse selects in (RC-020-14): iced's `rich_text`
//! is drawn as it is, links and all, and a paragraph of its own, laid out as `rich_text` lays
//! out its text, finds the byte under the mouse. That paragraph is made on the first press or
//! drag over the block and kept while width and text stay; the drawn one is never touched.

use std::sync::Arc;

use iced::{
    Element, Font, Length, Pixels, Point, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout,
        mouse::{self, Click},
        renderer,
        text::{self, Paragraph as _, Renderer as _},
        widget::{Operation, Tree, tree},
    },
    alignment,
    event::Event,
    keyboard,
    widget::text::{LineHeight, Shaping, Wrapping},
};

use super::select::{Gesture, Span, Spot};

type Paragraph = <iced::Renderer as text::Renderer>::Paragraph;

/// Block `block` of the preview, drawn by `content` from `spans`.
pub struct Selectable<'a, Message> {
    pub content: Element<'a, Message>,
    pub spans: Arc<[Span]>,
    pub size: Pixels,
    /// The font `content` sets; the renderer's default when `None`.
    pub font: Option<Font>,
    pub block: usize,
    /// The button is down after a click: a move over this block extends the selection.
    pub dragging: bool,
    /// The selection started here: this block reports the button coming up.
    pub releases: bool,
    pub on_select: fn(Gesture) -> Message,
}

#[derive(Default)]
struct State {
    /// The width `content` was laid out in.
    width: f32,
    hit: Option<(f32, String, Paragraph)>,
    last_click: Option<Click>,
    modifiers: keyboard::Modifiers,
}

impl<Message> Selectable<'_, Message> {
    /// The byte under `point`, relative to the block: before the first line its start, past the
    /// last its end.
    fn offset(&self, state: &mut State, renderer: &iced::Renderer, point: Point) -> usize {
        let text: String = self.spans.iter().map(|span| span.text.as_ref()).collect();
        let len = text.len();
        let stale = state
            .hit
            .as_ref()
            .is_none_or(|(width, laid, _)| *width != state.width || *laid != text);
        if stale {
            let paragraph = Paragraph::with_spans(text::Text {
                content: &self.spans[..],
                bounds: Size::new(state.width, f32::INFINITY),
                size: self.size,
                line_height: LineHeight::default(),
                font: self.font.unwrap_or_else(|| renderer.default_font()),
                align_x: text::Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::default(),
            });
            state.hit = Some((state.width, text, paragraph));
        }
        let Some((_, _, paragraph)) = &state.hit else {
            return 0;
        };
        if point.y < 0.0 {
            return 0;
        }
        if point.y > paragraph.min_bounds().height {
            return len;
        }
        let x = point.x.max(0.0);
        paragraph
            .hit_test(Point::new(x, point.y))
            .map_or(len, |hit| hit.cursor().min(len))
    }

    fn spot(&self, state: &mut State, renderer: &iced::Renderer, at: Point) -> Spot {
        Spot {
            block: self.block,
            at: self.offset(state, renderer, at),
        }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Selectable<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        tree.state.downcast_mut::<State>().width = limits.max().width;
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
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
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
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
        if shell.is_event_captured() {
            return;
        }
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_in(bounds) else {
                    return;
                };
                let click = Click::new(at, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                let spot = self.spot(state, renderer, at);
                let extend = state.modifiers.shift();
                shell.publish((self.on_select)(Gesture::Press {
                    spot,
                    extend,
                    kind: click.kind(),
                }));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if self.dragging => {
                // Beyond the scrolled area the cursor is only levitating, and still counts.
                let Some(at) = cursor.land().position() else {
                    return;
                };
                if at.y < bounds.y || at.y > bounds.y + bounds.height {
                    return;
                }
                let relative = Point::new(at.x - bounds.x, at.y - bounds.y);
                let spot = self.spot(state, renderer, relative);
                shell.publish((self.on_select)(Gesture::Drag(spot)));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if self.dragging && self.releases =>
            {
                shell.publish((self.on_select)(Gesture::Release));
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let inner = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if inner == mouse::Interaction::None && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            inner
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
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
}

impl<'a, Message: 'a> From<Selectable<'a, Message>> for Element<'a, Message> {
    fn from(selectable: Selectable<'a, Message>) -> Self {
        Element::new(selectable)
    }
}
