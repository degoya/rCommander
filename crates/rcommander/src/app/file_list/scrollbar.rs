//! The file list's scroll bar (RC-010-23), as in Windows and Total Commander: a press on the
//! thumb holds it and the list follows the pointer live until the button is released, also while
//! the pointer has left the bar; a click on the track above or below the thumb scrolls a screen
//! that way. The thumb is drawn stronger under the pointer and while held (`theme::thumb_fill`).
//!
//! The bar keeps only the grip in progress. Where the list stands is the pane's first row, which
//! the wheel and the keys move too: the bar draws what the pane says and sends where the pointer
//! wants it (`Message::ScrollTo`, `Pane::scroll_to`). A gesture of the window, as the wheel is,
//! not a command. The brief view scrolls by whole columns over the same range as the wheel.
//!
//! The arithmetic is [`Extent`], pure and tested without a window; the widget only measures.

use iced::{
    Border, Element, Length, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
    event::Event,
};
use rc_engine::PanelId;

use super::columns::SCROLLBAR_WIDTH;
use crate::{
    app::panel::Pane,
    theme::{self, Tokens},
};

/// The shortest the thumb gets, so a long list's thumb stays something to take hold of.
pub(super) const MIN_THUMB: f32 = 16.0;
/// The space between the bar's ends and the track the thumb runs in.
const END_GAP: f32 = 4.0;
/// The space between the bar's sides and the thumb.
const SIDE_GAP: f32 = 1.0;

/// Where a list stands, in rows: what the bar draws and what a drag or a click asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Extent {
    /// The first row shown.
    pub top: usize,
    /// The rows one screen shows: a page, every column's rows in the brief view.
    pub page: usize,
    /// All rows, the `..` row included.
    pub total: usize,
    /// The furthest first row ([`Pane::max_top`]).
    pub max_top: usize,
    /// What the first row moves by: one row, a column's rows in the brief view.
    pub step: usize,
}

/// Where a press on the bar landed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Hit {
    /// The track above the thumb: a screen back.
    Above,
    /// The thumb, this far below its upper edge: the drag keeps it there under the pointer.
    Thumb(f32),
    /// The track below the thumb: a screen on.
    Below,
}

impl Extent {
    pub fn of(pane: &Pane) -> Self {
        Self {
            top: pane.top(),
            page: pane.page_rows() * pane.columns(),
            total: pane.rows(),
            max_top: pane.max_top(),
            step: if pane.brief() { pane.page_rows() } else { 1 },
        }
    }

    /// Whether there is anything to scroll: a list longer than a screen.
    pub fn scrolls(&self) -> bool {
        self.max_top > 0
    }

    /// The thumb in a track `track` long: its offset from the track's start and its length, a
    /// screen's share of the list but no shorter than [`MIN_THUMB`].
    pub fn thumb(&self, track: f32) -> (f32, f32) {
        let track = track.max(0.0);
        let share = self.page as f32 / self.total.max(1) as f32;
        let length = (track * share).clamp(MIN_THUMB.min(track), track);
        let at = if self.max_top == 0 {
            0.0
        } else {
            self.top.min(self.max_top) as f32 / self.max_top as f32
        };
        ((track - length) * at, length)
    }

    /// Where a press `y` below the track's start lands in a track `track` long.
    pub fn hit(&self, track: f32, y: f32) -> Hit {
        let (offset, length) = self.thumb(track);
        if y < offset {
            Hit::Above
        } else if y >= offset + length {
            Hit::Below
        } else {
            Hit::Thumb(y - offset)
        }
    }

    /// The first row for the thumb's upper edge at `start` in a track `track` long: in
    /// proportion, held to the list, on a whole step.
    pub fn top_at(&self, track: f32, start: f32) -> usize {
        let (_, length) = self.thumb(track);
        let room = track - length;
        if room <= 0.0 {
            return self.top;
        }
        let step = self.step.max(1);
        let share = (start / room).clamp(0.0, 1.0);
        let steps = (share * self.max_top as f32 / step as f32).round() as usize;
        (steps * step).min(self.max_top)
    }

    /// A click on the track above the thumb: a screen back.
    pub fn page_back(&self) -> usize {
        self.top.saturating_sub(self.page)
    }

    /// A click on the track below the thumb: a screen on, not past the last.
    pub fn page_on(&self) -> usize {
        self.top.saturating_add(self.page).min(self.max_top)
    }
}

/// The bar of the list of `side`.
pub(super) struct ScrollBar<M> {
    extent: Extent,
    side: PanelId,
    tokens: &'static Tokens,
    /// The message asking for the view from a row.
    on_scroll: fn(PanelId, usize) -> M,
}

impl<M> ScrollBar<M> {
    pub fn new(
        pane: &Pane,
        side: PanelId,
        tokens: &'static Tokens,
        on_scroll: fn(PanelId, usize) -> M,
    ) -> Self {
        Self {
            extent: Extent::of(pane),
            side,
            tokens,
            on_scroll,
        }
    }

    /// Asks for the view from `top`, unless that is where it was last asked for.
    fn scroll(&self, shell: &mut Shell<'_, M>, top: usize, sent: usize) -> usize {
        if top != sent {
            shell.publish((self.on_scroll)(self.side, top));
        }
        top
    }
}

/// What the bar remembers between events.
#[derive(Debug, Default)]
struct State {
    grip: Option<Grip>,
    /// Whether the pointer is over the thumb, as last drawn.
    hovered: bool,
}

/// The thumb held.
#[derive(Debug, Clone, Copy)]
struct Grip {
    /// Where the press took the thumb, below its upper edge.
    anchor: f32,
    /// The first row last asked for, so a move within one row sends nothing.
    sent: usize,
}

/// The part of the bar the thumb runs in.
fn track(bounds: Rectangle) -> Rectangle {
    Rectangle {
        x: bounds.x + SIDE_GAP,
        y: bounds.y + END_GAP,
        width: (bounds.width - 2.0 * SIDE_GAP).max(0.0),
        height: (bounds.height - 2.0 * END_GAP).max(0.0),
    }
}

impl<M, Renderer> Widget<M, Theme, Renderer> for ScrollBar<M>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SCROLLBAR_WIDTH), Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, SCROLLBAR_WIDTH, Length::Fill)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let track = track(bounds);
        let extent = self.extent;
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_over(bounds).filter(|_| extent.scrolls()) else {
                    return;
                };
                match extent.hit(track.height, at.y - track.y) {
                    Hit::Thumb(anchor) => {
                        state.grip = Some(Grip {
                            anchor,
                            sent: extent.top,
                        });
                    }
                    Hit::Above => {
                        self.scroll(shell, extent.page_back(), extent.top);
                    }
                    Hit::Below => {
                        self.scroll(shell, extent.page_on(), extent.top);
                    }
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // Held, the thumb follows the pointer wherever it is, in the bar or not.
                if let Some(grip) = state.grip.as_mut() {
                    if let Some(at) = cursor.land().position() {
                        let start = at.y - track.y - grip.anchor;
                        let top = extent.top_at(track.height, start);
                        grip.sent = self.scroll(shell, top, grip.sent);
                    }
                    shell.capture_event();
                    return;
                }
                let hovered = extent.scrolls()
                    && cursor.position_over(bounds).is_some_and(|at| {
                        matches!(extent.hit(track.height, at.y - track.y), Hit::Thumb(_))
                    });
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.grip.is_some() =>
            {
                state.grip = None;
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorLeft) if state.hovered => {
                state.hovered = false;
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        if !self.extent.scrolls() {
            return;
        }
        let track = track(layout.bounds());
        let (offset, length) = self.extent.thumb(track.height);
        let state = tree.state.downcast_ref::<State>();
        let thumb = Rectangle {
            y: track.y + offset,
            height: length,
            ..track
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: thumb,
                border: Border::default().rounded(2.0),
                ..renderer::Quad::default()
            },
            theme::thumb_fill(self.tokens, state.hovered, state.grip.is_some()),
        );
    }
}

impl<'a, M: 'a, Renderer> From<ScrollBar<M>> for Element<'a, M, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(bar: ScrollBar<M>) -> Self {
        Element::new(bar)
    }
}

#[cfg(test)]
#[path = "scrollbar_tests.rs"]
mod tests;
