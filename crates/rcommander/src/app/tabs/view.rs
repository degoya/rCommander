//! Drawing the tab rows, the popups and the hotlist dialog - a card over the main window, or a
//! window of its own (RC-080-23, `dialog_windows.rs`).

use iced::{
    Alignment, Element, Length, Padding,
    mouse::Interaction,
    widget::{
        Row, Space, button, center, column, container, mouse_area, opaque, pick_list, responsive,
        row, scrollable, stack, text, text_input,
    },
};
use rc_engine::{PanelId, TabView, shown_path};

use super::{DragMsg, TabsMessage, editor::HotlistEditor, overflow, popup::Popup, reorder::gap_at};
use crate::{
    app::{App, Message, dialog_windows::DialogWindow},
    i18n::t,
    theme::{
        self, Tokens, fit, focus,
        fonts::{self, AreaFont},
        layout,
    },
};

/// Where a popup starts below the window's top: under the drive bar, the drive line and the tabs.
const POPUP_TOP: f32 = 92.0;
const POPUP_WIDTH: f32 = 420.0;
const POPUP_MAX_HEIGHT: f32 = 440.0;
/// A tab's title is a point smaller than the list it belongs to (12 beside the default 13).
const SMALLER: f32 = 1.0;
/// A tab's title is shortened in the middle beyond this width, at the default list font.
const TAB_TITLE_WIDTH: f32 = 160.0;
/// The indentation of one hotlist level.
const INDENT: f32 = 14.0;
/// The button after the tabs that fit, when not all do: the list of all tabs.
const MORE: &str = "\u{bb}";

fn wrap(message: TabsMessage) -> Message {
    Message::Tabs(message)
}

/// `panel`'s tab row, when it has two tabs or more: as many tabs as fit the panel's width, the
/// active one always among them, then `»` listing them all when some are left out (RC-010-21).
/// While one of its tabs is dragged, a bar marks where it would go, and the pointer leaving the
/// row cancels the drag (RC-010-24).
pub(in crate::app) fn tab_bar<'a>(
    app: &'a App,
    panel: PanelId,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let strip = &app.tabs.strips[panel.index()];
    if strip.len() < 2 {
        return None;
    }
    let font = tab_font();
    let drag = app.tabs.drag.filter(|drag| drag.panel == panel);
    let dragging = drag.is_some_and(|drag| drag.dragging);
    let marker = drag.and_then(|drag| drag.gap);
    let dark = app.dark;
    let tabs = responsive(move |size| {
        let titles: Vec<(String, bool)> = strip.iter().map(|tab| title(tab, font)).collect();
        let widths: Vec<f32> = titles
            .iter()
            .map(|(title, _)| overflow::tab_width(title, font))
            .collect();
        let active = strip.iter().position(|tab| tab.active).unwrap_or(0);
        let more = overflow::tab_width(MORE, font);
        let shown = overflow::visible_range(&widths, active, size.width, more);
        let hidden = shown.len() < strip.len();
        let mut faces: Vec<Element<'_, Message>> = strip
            .iter()
            .zip(titles)
            .zip(widths)
            .enumerate()
            .skip(shown.start)
            .take(shown.len())
            .map(|(index, ((tab, (title, shortened)), width))| {
                let face = TabFace {
                    title,
                    shortened,
                    index,
                    width,
                    dragging,
                };
                // Each tab in the theme of its own place: the active one in its panel's, another
                // on a server in that connection's (RC-100-51).
                let look = if tab.active {
                    tokens
                } else {
                    theme::themes::of_path(&tab.path, dark)
                };
                tab_button(panel, tab, face, look)
            })
            .collect();
        if dragging && let Some(gap) = marker.filter(|gap| shown.contains(gap) || *gap == shown.end)
        {
            faces.insert(gap - shown.start, drop_marker(tokens));
        }
        let mut line = Row::with_children(faces)
            .spacing(overflow::SPACING)
            .align_y(Alignment::End);
        if hidden {
            line = line.push(more_button(panel, font, tokens));
        }
        container(line).width(Length::Fill).clip(true).into()
    })
    .height(Length::Shrink);
    let bar = container(tabs)
        .width(Length::Fill)
        .clip(true)
        .padding(Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 8.0,
        });
    // Always the same widgets, so a press does not rebuild the row under the pointer.
    let mut bar = mouse_area(bar);
    if drag.is_some() {
        bar = bar.on_exit(wrap(TabsMessage::Drag(DragMsg::Cancel)));
    }
    Some(bar.into())
}

/// Where a dragged tab would go: an accent bar between two tabs.
fn drop_marker<'a>(tokens: &'static Tokens) -> Element<'a, Message> {
    container(Space::new().width(2.0).height(Length::Fill))
        .style(move |_| container::Style {
            background: Some(tokens.accent.into()),
            ..container::Style::default()
        })
        .into()
}

/// A tab's font: the list's (RC-010-17), a point smaller.
fn tab_font() -> AreaFont {
    let list = fonts::list();
    AreaFont {
        size: list.size - SMALLER,
        ..list
    }
}

/// The title `tab` shows in `font` and whether it was shortened: a long folder name keeps its
/// start and end; the lock mark stays whole.
fn title(tab: &TabView, font: AreaFont) -> (String, bool) {
    let list = fonts::list();
    let name = font.shorten(&tab.title, TAB_TITLE_WIDTH * list.column_scale());
    let shortened = name != tab.title.as_str();
    let title = if tab.locked {
        t!("tabs.locked", title = name)
    } else {
        name.into_owned()
    };
    (title, shortened)
}

/// What a tab shows and where it stands in its row.
struct TabFace {
    title: String,
    /// Its title is shortened: the whole path is its tooltip.
    shortened: bool,
    /// Its place in the row.
    index: usize,
    /// Its estimated width (`overflow.rs`).
    width: f32,
    /// One of the row's tabs is being dragged.
    dragging: bool,
}

fn tab_button<'a>(
    panel: PanelId,
    tab: &'a TabView,
    face: TabFace,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let font = tab_font();
    let active = tab.active;
    let TabFace {
        title,
        shortened,
        index,
        width,
        dragging,
    } = face;
    // The press is the label's, which takes it before the button: a click shows the tab on the
    // release, a press that moves far enough drags it (`reorder.rs`, RC-010-24).
    let drag = |message| wrap(TabsMessage::Drag(message));
    // focus-ring: pointer only (a tab of the tab bar, which Ctrl+Tab moves through)
    let mut label = mouse_area(container(small(title, font)).padding([4.0, overflow::PADDING]))
        .on_press(drag(DragMsg::Press(panel, tab.id)))
        // A click quicker than the pointer's subscription still ends here.
        .on_release(drag(DragMsg::Released))
        .on_middle_press(wrap(TabsMessage::Close(panel, tab.id)))
        .on_right_press(wrap(TabsMessage::Menu(panel, tab.id)))
        .interaction(if dragging {
            Interaction::Grabbing
        } else {
            Interaction::Pointer
        });
    if dragging {
        label = label.on_move(move |at| drag(DragMsg::Over(panel, gap_at(index, at.x, width))));
    }
    // A tab, not a button (owner, 2026-10-18): `theme::tab` joins the active one to the list. The
    // button draws it and its hover only; its own press never comes, the label took it.
    let face = button(label)
        .padding(0)
        .style(move |_, status| theme::tab(tokens, active, status))
        .on_press(wrap(TabsMessage::Select(panel, tab.id)));
    // The whole path only for a shortened title (owner, 2026-10-18).
    if shortened {
        fit::tip(face, shown_path(&tab.path).display().to_string(), tokens)
    } else {
        face.into()
    }
}

/// `»`: the list of all of `panel`'s tabs, in the tabs' look.
fn more_button<'a>(
    panel: PanelId,
    font: AreaFont,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let face = button(small(MORE, font))
        .padding([4.0, overflow::PADDING])
        .style(move |_, status| theme::tab(tokens, false, status))
        .on_press(wrap(TabsMessage::List(panel)));
    fit::tip(face, t!("tabs.more").to_owned(), tokens)
}

/// `base` with the open popup or the hotlist dialog over it.
pub(in crate::app) fn with_tabs_overlay<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    // In a window of its own (RC-080-23), the hotlist dialog draws there.
    let editor = app.tabs.editor.as_ref();
    if let Some(editor) = editor.filter(|_| !app.windowed(DialogWindow::Hotlist)) {
        // Marked as a dialog card: Tab walks its focus ring (`app/field_walk.rs`).
        let card = container(editor_card(editor, tokens))
            .id(theme::DIALOG_ID)
            .padding(16)
            .width(720)
            .height(480)
            .style(move |_| theme::dialog(tokens));
        let layer = center(card).style(move |_| theme::backdrop(tokens));
        return stack![base, opaque(layer)].into();
    }
    let Some(popup) = &app.tabs.popup else {
        return base;
    };
    let card = container(popup_card(popup, tokens)).width(Length::FillPortion(1));
    let beside = Space::new().width(Length::FillPortion(1));
    let placed = match popup.panel {
        PanelId::Left => row![card, beside],
        PanelId::Right => row![beside, card],
    };
    // focus-ring: pointer only (the layer that closes the menu)
    let layer = mouse_area(
        container(placed)
            .padding(Padding {
                top: POPUP_TOP,
                right: 16.0,
                bottom: 16.0,
                left: 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(wrap(TabsMessage::ClosePopup));
    stack![base, layer].into()
}

fn popup_card<'a>(popup: &'a Popup, tokens: &'static Tokens) -> Element<'a, Message> {
    let rows = popup.rows.iter().enumerate().map(|(index, entry)| {
        let heading = entry.action.is_none();
        let label = fit::line(entry.label.as_str()).color(if heading {
            tokens.text_secondary
        } else {
            tokens.text
        });
        // A tab's path takes two thirds of the row, shortened in the middle when it must be.
        let mut line = row![
            Space::new().width(INDENT * entry.depth as f32),
            label.width(Length::FillPortion(1)),
        ]
        .spacing(12)
        .padding([0, 8])
        .align_y(Alignment::Center);
        if !entry.detail.is_empty() {
            line = line.push(fit::path_cell(entry.detail.as_str(), 2, true, tokens));
        }
        // The key that runs the same, right-aligned as in a Windows menu (RC-010-24).
        if let Some(key) = &entry.key {
            line = line.push(fit::line(key.as_str()).color(tokens.text_secondary));
        }
        let cursor = index == popup.cursor;
        let current = entry.current;
        let face = container(line)
            .height(theme::ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| theme::row(tokens, cursor, true, current));
        // focus-ring: pointer only (a row of the tab menu, which the arrows move through)
        mouse_area(face)
            .on_press(wrap(TabsMessage::Pick(index)))
            .interaction(if heading {
                Interaction::Idle
            } else {
                Interaction::Pointer
            })
            .into()
    });
    container(scrollable(column(rows).padding(4)).id(super::POPUP_SCROLL_ID))
        .width(POPUP_WIDTH)
        .max_height(POPUP_MAX_HEIGHT)
        .style(move |_| theme::dialog(tokens))
        .into()
}

fn editor_card<'a>(editor: &'a HotlistEditor, tokens: &'static Tokens) -> Element<'a, Message> {
    column![
        Message::titled_head(t!("hotlist.dialog_title")).view(tokens),
        editor_body(editor, tokens),
    ]
    .spacing(12)
    .into()
}

/// The hotlist dialog's list, fields and buttons: on its card, or filling a window of its own.
pub(in crate::app) fn editor_body<'a>(
    editor: &'a HotlistEditor,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let rows = editor
        .items
        .iter()
        .zip(&editor.depths)
        .enumerate()
        .map(|(index, (item, depth))| {
            let chosen = editor.chosen == Some(item.id);
            let label = if item.folder {
                t!("hotlist.folder_row", name = item.name)
            } else {
                item.name.clone()
            };
            let line = row![
                Space::new().width(INDENT * *depth as f32),
                fit::line(label).width(Length::Fill),
            ]
            .padding([0, 8])
            .align_y(Alignment::Center);
            let face = container(line)
                .height(theme::ROW_HEIGHT)
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .clip(true)
                .style(move |_| theme::row(tokens, chosen, true, false));
            // focus-ring: pointer only (a row of the hotlist entries pick stop)
            mouse_area(face)
                .on_press(wrap(TabsMessage::Choose(index)))
                .interaction(Interaction::Pointer)
                .into()
        });
    let list = container(scrollable(column(rows).padding(4)))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, false));
    // The entries are one stop of the focus ring; the arrows choose (RC-080-16).
    let at = editor
        .chosen
        .and_then(|id| editor.items.iter().position(|item| item.id == id));
    let indices: Vec<usize> = (0..editor.items.len()).collect();
    let list = focus::pick(list, "hotlist.entries", &indices, at.as_ref(), |index| {
        wrap(TabsMessage::Choose(index))
    });
    let chosen = editor.chosen_item();
    let entry = chosen.is_some_and(|item| !item.folder);
    let field = |label: &'static str, value: &'a str, on: fn(String) -> TabsMessage, enabled| {
        let mut input = text_input("", value)
            .font(theme::UI_FONT)
            .size(theme::TEXT_SIZE)
            .padding([5, 8]);
        if enabled {
            input = input.on_input(move |text| wrap(on(text)));
        }
        column![
            layout::caption(label, None),
            layout::keyed(focus::Stop::field(input, label), label)
        ]
        .spacing(4)
    };
    let mut form = column![
        field(
            t!("hotlist.name"),
            &editor.name,
            TabsMessage::Name,
            chosen.is_some()
        ),
        field(t!("hotlist.path"), &editor.path, TabsMessage::Path, entry),
        field(
            t!("hotlist.target"),
            &editor.target,
            TabsMessage::Target,
            entry
        ),
        row![
            action(t!("hotlist.save"), chosen.map(|_| TabsMessage::Save)),
            action(t!("hotlist.up"), chosen.map(|_| TabsMessage::Up)),
            action(t!("hotlist.down"), chosen.map(|_| TabsMessage::Down)),
            action(t!("hotlist.delete"), chosen.map(|_| TabsMessage::Delete)),
        ]
        .spacing(6),
    ]
    .spacing(10)
    .width(Length::FillPortion(1));
    if chosen.is_some() {
        let folders = editor.folders();
        let current = editor.chosen_folder();
        let choose = |choice| wrap(TabsMessage::MoveTo(choice));
        let moves = focus::moves(&folders, current.as_ref(), &choose);
        let move_to = pick_list(folders, current, choose)
            .text_size(theme::TEXT_SIZE)
            .width(Length::Fill);
        let label = t!("hotlist.move_to");
        let move_to = layout::keyed(focus::Stop::pick(move_to, label, moves), label);
        form = form.push(column![layout::caption(label, None), move_to].spacing(4));
    }
    let buttons = row![
        action(
            t!("hotlist.add_current_button"),
            Some(TabsMessage::AddCurrent)
        ),
        action(t!("hotlist.new_folder"), Some(TabsMessage::AddFolder)),
        Space::new().width(Length::Fill),
        action(t!("hotlist.close"), Some(TabsMessage::CloseEditor)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    column![row![list, form].spacing(12).height(Length::Fill), buttons]
        .spacing(12)
        .height(Length::Fill)
        .into()
}

/// A button that sends `message`, greyed without one; a stop of the focus ring with the access
/// key its label marks.
fn action<'a>(label: &'a str, message: Option<TabsMessage>) -> Element<'a, Message> {
    let message = message.map(wrap);
    let face = button(layout::marked_icon_button(None, label))
        .padding([5, 12])
        .style(button::secondary)
        .on_press_maybe(message.clone());
    layout::keyed(focus::Stop::button(face, label, message), label)
}

fn small<'a>(content: impl text::IntoFragment<'a>, font: AreaFont) -> text::Text<'a> {
    text(content)
        .size(font.size)
        .font(font.font)
        .wrapping(text::Wrapping::None)
}
