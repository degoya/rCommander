//! Drawing a panel's terminal: the bar of its sessions over the terminal widget, or the shell
//! picker, or the question before a multi-line paste in the widget's place.

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, mouse_area, row, text},
};
use rc_engine::{PanelId, PanelView, TerminalInfo};
use rc_term_widget::TerminalView;

use super::{
    App, Message,
    terminal_pane::{TermMessage, widget_message},
};
use crate::{
    i18n::t,
    term,
    theme::{self, Tokens},
};

impl App {
    /// The body of `side` when it shows its terminal or the shell picker; `None` for its files.
    pub(super) fn terminal_body(
        &self,
        side: PanelId,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let picker = self.terms.picker.filter(|picker| picker.panel == side);
        if let Some(picker) = picker {
            return Some(picker_view(picker.selected, tokens));
        }
        if self.pane(side).state.view != PanelView::Terminal {
            return None;
        }
        let active = side == self.active;
        let tabs = self.engine.panel_terminals(side);
        let shown = self.shown_terminal(side);
        let body: Element<'_, Message> = match (&self.terms.paste, shown) {
            (Some((waiting, pasted)), Some(shown)) if *waiting == shown => {
                paste_view(pasted, tokens)
            }
            _ => match shown.and_then(|terminal| self.terms.widgets.get(&terminal)) {
                Some(widget) => container(TerminalView::show(widget).map(widget_message))
                    .padding(4)
                    .into(),
                None => Space::new().width(Length::Fill).height(Length::Fill).into(),
            },
        };
        Some(column![tab_bar(side, tabs, active, tokens), body].into())
    }
}

/// The sessions of a panel, the shown one marked, each with a close mark; `+` opens another.
fn tab_bar(
    side: PanelId,
    tabs: Vec<TerminalInfo>,
    active: bool,
    tokens: &'static Tokens,
) -> Element<'static, Message> {
    let mut items: Vec<Element<'static, Message>> = Vec::with_capacity(tabs.len() + 1);
    for tab in tabs.into_iter().filter(|tab| !tab.state.is_exited()) {
        let shown = tab.shown;
        let title = mouse_area(
            container(label(tab.title))
                .padding([3, 8])
                .style(move |_| theme::row(tokens, shown, active, false)),
        )
        .on_press(Message::Terminal(TermMessage::Select(side, tab.terminal)));
        let close = mouse_area(
            container(label(t!("terminal.close_tab")).color(tokens.text_secondary)).padding([3, 6]),
        )
        .on_press(Message::Terminal(TermMessage::Close(side, tab.terminal)));
        items.push(row![title, close].align_y(Alignment::Center).into());
    }
    items.push(button(
        t!("terminal.new_tab"),
        TermMessage::Choose(side),
        tokens,
    ));
    column![
        container(row(items).spacing(4).align_y(Alignment::Center))
            .padding([4, 8])
            .width(Length::Fill),
        divider(tokens),
    ]
    .into()
}

/// The shells to start, numbered; the arrows' row marked.
fn picker_view(selected: usize, tokens: &'static Tokens) -> Element<'static, Message> {
    let rows = term::shells().iter().enumerate().map(|(index, profile)| {
        let line = t!(
            "terminal.shell_row",
            number = index + 1,
            name = term::shell_label(profile)
        );
        let chosen = index == selected;
        mouse_area(
            container(label(line))
                .padding([4, 12])
                .width(Length::Fill)
                .style(move |_| theme::row(tokens, chosen, true, false)),
        )
        .on_press(Message::Terminal(TermMessage::Pick(index)))
        .into()
    });
    column![
        label(t!("terminal.choose_shell")).font(theme::UI_FONT_STRONG),
        column(rows).spacing(2),
        label(t!("terminal.choose_hint")).color(tokens.text_secondary),
        button(t!("terminal.cancel"), TermMessage::CancelPick, tokens),
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The question before text with line breaks, or with control characters, reaches the shell.
fn paste_view(pasted: &str, tokens: &'static Tokens) -> Element<'static, Message> {
    column![
        label(paste_question(pasted)),
        row![
            button(t!("terminal.paste"), TermMessage::PasteConfirmed, tokens),
            button(t!("terminal.cancel"), TermMessage::PasteCancelled, tokens),
        ]
        .spacing(8),
        label(t!("terminal.paste_hint")).color(tokens.text_secondary),
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn button(
    caption: impl text::IntoFragment<'static>,
    message: TermMessage,
    tokens: &'static Tokens,
) -> Element<'static, Message> {
    mouse_area(
        container(label(caption))
            .padding([4, 12])
            .style(move |_| theme::key_button(tokens)),
    )
    .on_press(Message::Terminal(message))
    .into()
}

fn label(content: impl text::IntoFragment<'static>) -> text::Text<'static> {
    text(content)
        .size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .wrapping(text::Wrapping::None)
}

fn divider(tokens: &'static Tokens) -> Element<'static, Message> {
    container(Space::new().width(Length::Fill).height(1))
        .style(move |_| theme::divider(tokens))
        .into()
}

/// What the paste question asks about `pasted`: the lines it runs when it has a line break, else
/// the control characters (Ctrl+C and the like) that act on the program (RC-080-51) - one line
/// is no "1 lines" question.
pub(super) fn paste_question(pasted: &str) -> String {
    if pasted.contains(['\n', '\r']) {
        let lines = pasted.lines().count().max(1);
        t!("terminal.paste_question", lines = lines)
    } else {
        t!("terminal.paste_control").to_owned()
    }
}
