//! Where the drive menu and a drive's context menu open (RC-100-62): under the button they were
//! opened from - a bar button, `»`, a folder's button, the drive line's drive - kept inside the
//! window. The button carries a container id ([`id`]); opening the menu measures it ([`measure`])
//! and the card shows once the answer is in. Opened by the keyboard (Alt+F1, Alt+F2) there is no
//! button, and the card stands where it always did, over its panel's half under the bars.

use iced::{
    Element, Length, Rectangle, Size, Task,
    advanced::widget::{Id, Operation, operate, operation::Outcome},
    widget::{Space, container, mouse_area, pin, responsive, row},
};
use rc_engine::PanelId;

use super::DriveMessage;
use crate::{app::Message, theme};

/// Where a menu opened by the keyboard starts below the window's top: under the bar and the
/// drive line.
const FIXED_TOP: f32 = 70.0;
/// The space between a button's lower edge and the card under it.
const GAP: f32 = 2.0;
/// The space a card keeps from the window's edges.
const MARGIN: f32 = 8.0;
/// A card's padding and border above and below its lines.
const CARD_FRAME: f32 = 2.0 * 4.0 + 2.0;

/// Where the open menus stand.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Anchor {
    /// Opened by the keyboard: over the panel's half under the bars.
    #[default]
    Fixed,
    /// The button it was opened from is being measured; nothing shows until it is.
    Measuring,
    /// Under the button with these bounds in the window.
    Under(Rectangle),
}

/// A button a menu opens from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button<'a> {
    /// The drive line's drive.
    Line,
    /// The bar's `»`.
    More,
    /// A folder's button, by the folder's path from the root (RC-100-63).
    Folder(&'a [String]),
    /// A drive's button, by the drive's id.
    Drive(&'a str),
}

/// The container id of `button` in the bar or drive line of `panel`.
pub fn id(panel: PanelId, button: Button<'_>) -> Id {
    let side = match panel {
        PanelId::Left => "left",
        PanelId::Right => "right",
    };
    let what = match button {
        Button::Line => "line".to_owned(),
        Button::More => "more".to_owned(),
        // A name holds no control character (`rc-db`'s check), so the separator is unambiguous.
        Button::Folder(path) => format!("folder.{}", path.join("\u{1f}")),
        Button::Drive(drive) => format!("drive.{drive}"),
    };
    Id::from(format!("rcommander.drive-bar.{side}.{what}"))
}

/// The bounds of the container `id`, as [`DriveMessage::Anchored`].
pub fn measure(id: Id) -> Task<Message> {
    operate(Bounds { id, found: None }).map(|bounds| Message::Drive(DriveMessage::Anchored(bounds)))
}

/// A card's height for `lines` lines of the menu, at most `max`: what keeps it inside the
/// window's bottom.
pub fn card_height(lines: usize, max: f32) -> f32 {
    (lines.max(1) as f32 * theme::ROW_HEIGHT + CARD_FRAME).min(max)
}

/// Where a card of `card` size opens under `anchor` in a window of `area` size: at the button's
/// left edge, just under it; moved left to stay inside the right edge and up to stay inside the
/// bottom, [`MARGIN`] from either; never past the left or top edge. The rectangle's size is the
/// card's, cut to the window.
pub fn place(anchor: Rectangle, card: Size, area: Size) -> Rectangle {
    let width = card.width.min(area.width - 2.0 * MARGIN).max(0.0);
    let height = card.height.min(area.height - 2.0 * MARGIN).max(0.0);
    let x = anchor.x.min(area.width - MARGIN - width).max(MARGIN);
    let y = (anchor.y + anchor.height + GAP)
        .min(area.height - MARGIN - height)
        .max(MARGIN);
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

/// The card `card` makes for its place (it is given the rectangle [`place`] found) under
/// `anchor`, over the whole window; a click beside it closes the menus.
pub fn layer<'a>(
    anchor: Rectangle,
    size: Size,
    card: impl Fn(Rectangle) -> Element<'a, Message> + 'a,
) -> Element<'a, Message> {
    responsive(move |area| {
        let at = place(anchor, size, area);
        let close = Message::Drive(DriveMessage::CloseMenu);
        // focus-ring: pointer only (the layer that closes the menu)
        mouse_area(
            pin(card(at))
                .x(at.x)
                .y(at.y)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(close.clone())
        .on_right_press(close)
        .into()
    })
    .into()
}

/// `card` where a menu opened by the keyboard stands: over the half of `panel`'s side, under the
/// bars, over the whole window; a click beside it closes the menus.
pub fn fixed<'a>(panel: PanelId, card: Element<'a, Message>) -> Element<'a, Message> {
    let card = container(card).width(Length::FillPortion(1));
    let beside = Space::new().width(Length::FillPortion(1));
    let placed = match panel {
        PanelId::Left => row![card, beside],
        PanelId::Right => row![beside, card],
    };
    let close = Message::Drive(DriveMessage::CloseMenu);
    // focus-ring: pointer only (the layer that closes the menu)
    mouse_area(
        container(placed)
            .padding(iced::Padding {
                top: FIXED_TOP,
                right: 16.0,
                bottom: 16.0,
                left: 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(close.clone())
    .on_right_press(close)
    .into()
}

/// A widget operation that finds the bounds of the container with `id`.
struct Bounds {
    id: Id,
    found: Option<Rectangle>,
}

impl Operation<Option<Rectangle>> for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<Rectangle>>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.id) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Option<Rectangle>> {
        Outcome::Some(self.found)
    }
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
