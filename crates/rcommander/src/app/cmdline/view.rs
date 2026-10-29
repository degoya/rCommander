//! Drawing: the command line under the panels with the active panel's folder as its prompt, the
//! quick search or filter in its place while one is typed, the Alt+F8 list above it, and the
//! filter's mark in a panel's path bar.

use iced::{
    Alignment, Element, Length,
    widget::{Column, container, mouse_area, row, scrollable, text, text_input},
};
use rc_engine::PanelState;

use super::{CMDLINE_ID, CmdLineMsg, Search, SearchKind, wrap};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{self, Tokens},
};

/// The tallest the Alt+F8 list grows before it scrolls.
const LIST_HEIGHT: f32 = 200.0;

fn input(text: String) -> Message {
    wrap(CmdLineMsg::Input(text))
}

impl App {
    /// The command line, or the search typed in its place, with the open history list above.
    pub(in crate::app) fn command_line(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        let line = match &self.cmdline.search {
            Some(search) => search_row(search, tokens),
            None => self.line_row(tokens),
        };
        match self.cmdline.list {
            Some(selected) => Column::new()
                .spacing(4)
                .push(history_list(&self.cmdline.history, selected, tokens))
                .push(line)
                .into(),
            None => line,
        }
    }

    /// The prompt - the active panel's folder - and the field; disabled with a hint outside a
    /// local folder.
    fn line_row(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        let enabled = self.line_enabled();
        let folder = self.pane(self.active).state.path.path.display().to_string();
        let placeholder = if enabled {
            ""
        } else {
            t!("cmdline.local_only")
        };
        let field = text_input(placeholder, &self.cmdline.text)
            .id(CMDLINE_ID)
            .on_input_maybe(enabled.then_some(input))
            .size(theme::TEXT_SIZE)
            .font(theme::UI_FONT)
            .padding([3, 6]);
        row![
            text(t!("cmdline.prompt", path = folder))
                .size(theme::TEXT_SIZE)
                .font(theme::UI_FONT)
                .color(tokens.text_secondary)
                .wrapping(text::Wrapping::None),
            mouse_area(field)
                .on_enter(wrap(CmdLineMsg::Hover(true)))
                .on_exit(wrap(CmdLineMsg::Hover(false))),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }
}

/// A quick search or filter as typed so far, in an accented box.
fn search_row(search: &Search, tokens: &'static Tokens) -> Element<'static, Message> {
    let label = match search.kind {
        SearchKind::Find => t!("quick_search.find"),
        SearchKind::Filter => t!("quick_search.filter"),
    };
    let typed = container(
        text(search.text.clone())
            .size(theme::TEXT_SIZE)
            .font(theme::UI_FONT_STRONG)
            .color(tokens.text),
    )
    .padding([3, 6])
    .width(Length::Fill)
    .style(move |_| theme::panel(tokens, true));
    row![
        text(label)
            .size(theme::TEXT_SIZE)
            .font(theme::UI_FONT)
            .color(tokens.accent),
        typed,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// The kept lines, newest first, the highlighted one in the accent colour; a click takes one.
fn history_list<'a>(
    history: &'a [String],
    selected: usize,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut lines = Column::new().spacing(2);
    if history.is_empty() {
        lines = lines.push(
            text(t!("cmdline.history_empty"))
                .size(theme::TEXT_SIZE)
                .color(tokens.text_secondary),
        );
    }
    for (index, line) in history.iter().enumerate() {
        let color = if index == selected {
            tokens.accent
        } else {
            tokens.text
        };
        let entry = text(line.as_str())
            .size(theme::TEXT_SIZE)
            .font(theme::UI_FONT)
            .color(color)
            .wrapping(text::Wrapping::None);
        lines = lines.push(mouse_area(entry).on_press(wrap(CmdLineMsg::Pick(index))));
    }
    container(
        scrollable(lines)
            .width(Length::Fill)
            .id(super::HISTORY_SCROLL_ID),
    )
    .padding([4, 8])
    .width(Length::Fill)
    .max_height(LIST_HEIGHT)
    .style(move |_| theme::panel(tokens, true))
    .into()
}

/// A panel's path bar with the filter in force beside it, if there is one.
pub(in crate::app) fn with_filter<'a>(
    bar: Element<'a, Message>,
    state: &PanelState,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(pattern) = &state.filter.pattern else {
        return bar;
    };
    let mark = text(t!("panel.filter_mark", pattern = pattern))
        .size(theme::TEXT_SIZE)
        .font(theme::UI_FONT_STRONG)
        .color(tokens.accent)
        .wrapping(text::Wrapping::None);
    row![bar, mark].spacing(8).align_y(Alignment::Center).into()
}
