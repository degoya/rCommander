//! The shared frame of a dialog (`docs/design.md`): the card in the small, the medium or the
//! large size, its head - the title and the close X at the top right (RC-080-15) - and the one
//! row of buttons at its bottom right, in Windows' and Total Commander's order, each button with
//! its action's icon before the label where it has one ([`icon_button`]). A dialog in a window
//! of its own has no head (the system's title bar has the title and the X): [`window_body`].
//! Every dialog button is a stop of the dialog's focus ring (`theme::focus`, RC-080-16); the X is
//! not, as the title bar's X is no stop in Windows. A button's label may mark its access key
//! with `&` (`theme::mnemonic`): the button underlines it, and Alt with the letter presses it;
//! a checkbox and the caption of a text field, a pick list or an option group read theirs in
//! [`access`].

mod access;

use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Pixels,
    advanced::widget::Id,
    widget::{Row, Space, button, center, column, container, opaque, rich_text, row, span, text},
};

use super::{
    DANGER, DIALOG_ID, LARGE_DIALOG_MARGIN, TEXT_SIZE, Tokens, UI_FONT, UI_FONT_STRONG, backdrop,
    dialog,
    fit::line,
    focus,
    icon::{self, Icon},
    mnemonic::{self, Label},
};

pub use access::{caption, check, keyed, marked_icon_button};

/// A question or a prompt: a few lines and its buttons.
pub const SMALL_DIALOG_WIDTH: f32 = 480.0;
/// A form or a progress window: as wide as the window allows up to this width, so a path line
/// gets the room there is.
pub const MEDIUM_DIALOG_WIDTH: f32 = 680.0;
/// The space between a dialog card's edge and its content.
pub const DIALOG_PADDING: f32 = 16.0;
/// A dialog button's padding, vertical and horizontal.
pub const BUTTON_PADDING: [u16; 2] = [6, 16];
/// The space between the buttons of a row.
pub const BUTTON_SPACING: f32 = 8.0;
/// The space between a button's icon and its label.
pub const ICON_SPACING: f32 = 6.0;

/// The space between a card's head and its content.
const HEAD_SPACING: f32 = 10.0;
/// The space between a large dialog's head and its content (`docs/design.md`: 12).
const LARGE_HEAD_SPACING: f32 = 12.0;
/// The X's padding around its glyph.
const CLOSE_PADDING: f32 = 4.0;

/// A dialog's head: its title at the left, the close X at the top right. The X sends `close` -
/// in the app what Escape sends, so a click on it does what Escape does: cancel, close, or stop
/// what runs first where Escape does that. Every dialog card takes one, so none is without.
pub struct Head<'a, Message> {
    title: Element<'a, Message>,
    close: Message,
}

impl<'a, Message: Clone + 'a> Head<'a, Message> {
    /// `title` as the dialog draws it (13 semibold on a small or medium card, 18 on a large one),
    /// and what the X sends.
    pub fn new(title: impl Into<Element<'a, Message>>, close: Message) -> Self {
        Self {
            title: title.into(),
            close,
        }
    }

    /// The row: the title takes the room, the X sits at the right end.
    pub fn view(self, tokens: &'static Tokens) -> Element<'a, Message> {
        row![
            container(self.title).width(Length::Fill),
            close_button(self.close, tokens),
        ]
        .spacing(BUTTON_SPACING)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
    }
}

/// `title` as most dialogs' heads draw it: one line in the strong font. A large dialog with
/// its title at 18, or a title coloured by a warning, builds its own.
pub fn dialog_title<'a, Message: 'a>(title: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    line(title).font(UI_FONT_STRONG).into()
}

/// The X of a dialog's head: the dismiss glyph without a fill, filled while the pointer is on it.
pub fn close_button<'a, Message: Clone + 'a>(
    on_press: Message,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let line = text::LineHeight::default().to_absolute(Pixels(TEXT_SIZE));
    button(
        text(Icon::Close.glyph())
            .font(icon::FONT)
            .size(icon::SIZE)
            .line_height(line),
    )
    .padding(CLOSE_PADDING)
    .style(move |_, status| close_style(tokens, status))
    .on_press(on_press)
    .into()
}

/// The X's look: the text colour on no fill; the inactive cursor's fill under the pointer.
pub fn close_style(tokens: &Tokens, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => Some(tokens.cursor_fill_inactive),
        button::Status::Active | button::Status::Disabled => None,
    };
    button::Style {
        background: fill.map(Background::Color),
        text_color: tokens.text,
        border: Border::default().rounded(4.0),
        ..button::Style::default()
    }
}

/// A small or medium dialog over the dimmed window that takes every click: `head` over
/// `content` in a card that is centred, at most `width` wide and narrower in a narrow window,
/// never closer to the window's edge than [`LARGE_DIALOG_MARGIN`].
pub fn dialog_card<'a, Message: Clone + 'a>(
    head: Head<'a, Message>,
    content: impl Into<Element<'a, Message>>,
    width: f32,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    warning_card(head, content, width, false, tokens)
}

/// [`dialog_card`] that, while `warning`, carries a 2-px border in the danger colour: a host key
/// or a certificate that changed.
pub fn warning_card<'a, Message: Clone + 'a>(
    head: Head<'a, Message>,
    content: impl Into<Element<'a, Message>>,
    width: f32,
    warning: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let body = column![head.view(tokens), content.into()].spacing(HEAD_SPACING);
    let card = container(body)
        .id(DIALOG_ID)
        .padding(DIALOG_PADDING)
        .width(Length::Fill)
        .max_width(width)
        .style(move |_| {
            let mut style = dialog(tokens);
            if warning {
                style.border = style.border.width(2.0).color(DANGER);
            }
            style
        });
    opaque(
        center(card)
            .padding(LARGE_DIALOG_MARGIN)
            .style(move |_| backdrop(tokens)),
    )
}

/// A large dialog - a list or a table to work in - over the dimmed window that takes every
/// click: `head` over `content` in a card that fills the window but for
/// [`LARGE_DIALOG_MARGIN`], so its list gets the room the window has. Questions and prompts stay
/// small cards of their own size.
pub fn large_dialog<'a, Message: Clone + 'a>(
    head: Head<'a, Message>,
    content: impl Into<Element<'a, Message>>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let body = column![head.view(tokens), content.into()]
        .spacing(LARGE_HEAD_SPACING)
        .height(Length::Fill);
    let card = container(body)
        .id(DIALOG_ID)
        .padding(DIALOG_PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| dialog(tokens));
    opaque(
        container(card)
            .padding(LARGE_DIALOG_MARGIN)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| backdrop(tokens)),
    )
}

/// A large dialog in a window of its own (RC-080-15): `content` fills the window on the card's
/// surface with the card's padding; the system's title bar carries the title and the X.
pub fn window_body<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    warning_window_body(content, false, tokens)
}

/// [`window_body`] that, while `warning`, carries the 2-px border in the danger colour that
/// [`warning_card`] gives a card: a host key or a certificate that changed, in a window of its own
/// (RC-080-23), whose title bar cannot be red.
pub fn warning_window_body<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    warning: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    container(content)
        .id(DIALOG_ID)
        .padding(DIALOG_PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| {
            let mut style = dialog(tokens);
            style.border = if warning {
                Border::default().width(2.0).color(DANGER)
            } else {
                Border::default()
            };
            style.shadow = iced::Shadow::default();
            style
        })
        .into()
}

/// Which card - a container marked [`DIALOG_ID`] - an operation over the widget tree is in:
/// cards are numbered from 1 as they are entered, so the last is the one on top (a question
/// over a dialog, or inside it). An operation reports each container to [`Cards::container`] and
/// calls [`Cards::enter`] in its `traverse`, before the children.
#[derive(Debug, Default)]
pub struct Cards {
    /// The cards entered so far: the number of the last one.
    pub entered: usize,
    /// The card the traversal is in.
    pub inside: Option<usize>,
    /// The container just reported is a marked card; its children come next.
    entering: bool,
}

impl Cards {
    pub fn container(&mut self, id: Option<&Id>) {
        self.entering = id == Some(&Id::new(DIALOG_ID));
    }

    /// Before the children of a container: a marked card's get its number. Returns the card
    /// the traversal was in, to go back to after the children, and whether this is a card.
    pub fn enter(&mut self) -> (Option<usize>, bool) {
        let outer = self.inside;
        let card = std::mem::take(&mut self.entering);
        if card {
            self.entered += 1;
            self.inside = Some(self.entered);
        }
        (outer, card)
    }
}

/// The default button of a dialog (Enter while no other button has the focus ring): filled in
/// the accent colour. `None` draws it disabled, and no stop of the ring.
pub fn primary_button<'a, Message: Clone + 'a>(
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    primary_icon_button(None, label, on_press)
}

/// [`primary_button`] with `icon` before its label.
pub fn primary_icon_button<'a, Message: Clone + 'a>(
    icon: impl Into<Option<Icon>>,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    dialog_button(icon, label, button::primary, on_press)
}

/// Any other button of a dialog: Cancel, Close, Apply, the extra actions.
pub fn secondary_button<'a, Message: Clone + 'a>(
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    secondary_icon_button(None, label, on_press)
}

/// [`secondary_button`] with `icon` before its label.
pub fn secondary_icon_button<'a, Message: Clone + 'a>(
    icon: impl Into<Option<Icon>>,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    dialog_button(icon, label, button::secondary, on_press)
}

/// A small secondary button that shows only `icon`, with `label` in a tooltip and as the name of
/// its stop: a move repeated per row (a login script's Up and Down, RC-050-13), where a label
/// would take the room the row needs.
pub fn glyph_button<'a, Message: Clone + 'a>(
    icon: Icon,
    label: &'a str,
    on_press: Option<Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let line_height = text::LineHeight::default().to_absolute(Pixels(TEXT_SIZE));
    let glyph = text(icon.glyph())
        .font(icon::FONT)
        .size(icon::SIZE)
        .line_height(line_height);
    let control = button(glyph)
        .padding(BUTTON_PADDING)
        .style(button::secondary)
        .on_press_maybe(on_press.clone());
    super::fit::tip(
        focus::Stop::button(control, label, on_press),
        label.to_owned(),
        tokens,
    )
}

/// A dialog button in `style`: `label` read for its access key (`theme::mnemonic`), which the
/// face underlines while it shows and the stop answers to.
fn dialog_button<'a, Message: Clone + 'a>(
    icon: impl Into<Option<Icon>>,
    label: &'a str,
    style: fn(&iced::Theme, button::Status) -> button::Style,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let marked = Label::parse(label);
    let letter = marked.letter;
    let control = button(face(icon, marked_label(marked, mnemonic::shown())))
        .padding(BUTTON_PADDING)
        .style(style)
        .on_press_maybe(on_press.clone());
    focus::Stop::button(control, label, on_press)
        .letter(letter)
        .into()
}

/// A button's face: `icon`, then `label`; the label alone without one. The icon is a glyph
/// without a colour of its own, so it draws in the button's text colour - the theme's in light
/// and dark, dimmed with the label when the button is disabled - and in the label's line height,
/// so a button with an icon is as tall as one without. Every button with an icon has this face,
/// also one in a style of its own.
pub fn icon_button<'a, Message: 'a>(
    icon: impl Into<Option<Icon>>,
    label: impl text::IntoFragment<'a>,
) -> Element<'a, Message> {
    face(icon, line(label).into())
}

fn face<'a, Message: 'a>(
    icon: impl Into<Option<Icon>>,
    label: Element<'a, Message>,
) -> Element<'a, Message> {
    let Some(icon) = icon.into() else {
        return label;
    };
    let line = text::LineHeight::default().to_absolute(Pixels(TEXT_SIZE));
    row![
        text(icon.glyph())
            .font(icon::FONT)
            .size(icon::SIZE)
            .line_height(line),
        label,
    ]
    .spacing(ICON_SPACING)
    .align_y(Alignment::Center)
    .into()
}

/// `label` without its mark, the access key underlined while it is `shown` ([`mnemonic::shown`])
/// - one text otherwise, so the label reads as before.
fn marked_label<'a, Message: 'a>(label: Label<'a>, shown: bool) -> Element<'a, Message> {
    tinted_label(label, shown, None)
}

/// [`marked_label`] in `color`; in the colour round it without one.
fn tinted_label<'a, Message: 'a>(
    label: Label<'a>,
    shown: bool,
    color: Option<Color>,
) -> Element<'a, Message> {
    if shown && let Some((before, key, after)) = label.split() {
        let spans: [text::Span<'a, (), Font>; 3] = [
            span(before.to_owned()),
            span(key.to_owned()).underline(true),
            span(after.to_owned()),
        ];
        return rich_text::<(), Message, iced::Theme, iced::Renderer>(spans)
            .size(TEXT_SIZE)
            .font(UI_FONT)
            .wrapping(text::Wrapping::None)
            .color_maybe(color)
            .into();
    }
    line(label.text).color_maybe(color).into()
}

/// A dialog's one row of buttons: `extra` on the left, then the space, then `primary` and the
/// `secondary` buttons at the right - `[Compare]      [Overwrite] [Cancel]`, `[OK] [Cancel]
/// [Apply]` as Windows and Total Commander order them.
pub fn button_row<'a, Message: 'a>(
    primary: Option<Element<'a, Message>>,
    secondary: impl IntoIterator<Item = Element<'a, Message>>,
    extra: impl IntoIterator<Item = Element<'a, Message>>,
) -> Element<'a, Message> {
    Row::with_children(extra)
        .push(Space::new().width(Length::Fill))
        .extend(primary)
        .extend(secondary)
        .spacing(BUTTON_SPACING)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
