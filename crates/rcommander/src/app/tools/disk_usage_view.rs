//! Drawing the disk-usage window (RC-100-98): the folder shown over its two tabs - the folders
//! with their size, share (a bar), files and folders, or the largest files -, where the walk
//! stands, and one button row. Names are cut in the middle; sizes follow `docs/design.md`.

use iced::{
    Alignment, Background, Element, Length,
    alignment::Horizontal,
    widget::{Column, Space, column, container, mouse_area, row, sensor},
};
use rc_engine::disk_usage::{FolderRow, SizeBy};

use super::disk_usage::{DiskUsageWindow, Stand, UsageMsg, UsageTab, wrap};
use crate::{
    app::Message,
    format,
    i18n::t,
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
        icon::Icon,
        layout,
    },
};

const SIZE_WIDTH: f32 = 96.0;
const SHARE_WIDTH: f32 = 130.0;
const BAR_WIDTH: f32 = 72.0;
const COUNT_WIDTH: f32 = 80.0;

/// The window's title.
pub fn title() -> &'static str {
    t!("disk_usage.title")
}

/// The window's content: its own window's whole, or the card's body.
pub fn body<'a>(window: &'a DiskUsageWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let shown = if window.relative.is_empty() {
        rc_engine::shown_path(&window.root).display().to_string()
    } else {
        rc_engine::shown_path(&window.root.join(&window.relative))
            .display()
            .to_string()
    };
    let tabs = row![
        tab(
            t!("disk_usage.tab_folders"),
            UsageTab::Folders,
            window.tab,
            tokens
        ),
        tab(
            t!("disk_usage.tab_largest"),
            UsageTab::Largest,
            window.tab,
            tokens
        ),
    ]
    .spacing(6);
    let tabs = focus::pick(
        tabs,
        "disk_usage.tabs",
        &[UsageTab::Folders, UsageTab::Largest],
        Some(&window.tab),
        |tab| wrap(UsageMsg::Tab(tab)),
    );
    let list = match window.tab {
        UsageTab::Folders => folder_list(window, tokens),
        UsageTab::Largest => file_list(window, tokens),
    };
    column![
        fit::path_line(shown, tokens.text, tokens),
        tabs,
        list,
        status(window, tokens),
        buttons(window),
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}

/// The window as a large card over the main window.
pub fn overlay<'a>(window: &'a DiskUsageWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    theme::large_dialog(Message::titled_head(title()), body(window, tokens), tokens)
}

/// A size by the setting `disk_usage.size`; a dash where the backend did not tell it.
fn size_of(by: SizeBy, usage: &rc_engine::DirUsage) -> String {
    by.of(usage)
        .map_or_else(|| t!("disk_usage.unknown").to_owned(), format::size)
}

fn folder_list<'a>(window: &'a DiskUsageWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let header = row![
        line(t!("disk_usage.column_name"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
        line(t!("disk_usage.column_size"))
            .color(tokens.text_secondary)
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
        line(t!("disk_usage.column_share"))
            .color(tokens.text_secondary)
            .width(SHARE_WIDTH),
        line(t!("disk_usage.column_files"))
            .color(tokens.text_secondary)
            .width(COUNT_WIDTH)
            .align_x(Horizontal::Right),
        line(t!("disk_usage.column_folders"))
            .color(tokens.text_secondary)
            .width(COUNT_WIDTH)
            .align_x(Horizontal::Right),
    ]
    .spacing(8)
    .padding([0, 6]);
    let by = window.choices.size_by;
    let rows = window.list.visible(window.rows.len()).filter_map(|index| {
        let entry = window.rows.get(index)?;
        let cells = row![
            fit::path_cell(name_of(entry), 1, entry.node.is_none(), tokens),
            line(size_of(by, &entry.usage))
                .width(SIZE_WIDTH)
                .align_x(Horizontal::Right),
            share(entry.share, tokens),
            line(format::count(entry.usage.files))
                .width(COUNT_WIDTH)
                .align_x(Horizontal::Right),
            line(format::count(entry.usage.dirs))
                .width(COUNT_WIDTH)
                .align_x(Horizontal::Right),
        ]
        .spacing(8)
        .padding([0, 6])
        .align_y(Alignment::Center);
        let cursor = index == window.list.cursor();
        Some(framed(
            cells,
            index,
            cursor,
            tokens,
            UsageMsg::Click(index),
            UsageMsg::Open(index),
        ))
    });
    listed(
        header,
        rows,
        "disk_usage.folders",
        UsageMsg::Scroll,
        UsageMsg::Viewport,
    )
}

fn file_list<'a>(window: &'a DiskUsageWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let header = row![
        line(t!("disk_usage.column_path"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
        line(t!("disk_usage.column_size"))
            .color(tokens.text_secondary)
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
    ]
    .spacing(8)
    .padding([0, 6]);
    let rows = window
        .files
        .visible(window.largest.len())
        .filter_map(|index| {
            let file = window.largest.get(index)?;
            let size = match window.choices.size_by {
                SizeBy::Bytes => Some(file.bytes),
                SizeBy::Allocated => file.allocated,
            };
            let size = size.map_or_else(|| t!("disk_usage.unknown").to_owned(), format::size);
            let cells = row![
                fit::path_cell(file.path.clone(), 1, false, tokens),
                line(size).width(SIZE_WIDTH).align_x(Horizontal::Right),
            ]
            .spacing(8)
            .padding([0, 6])
            .align_y(Alignment::Center);
            let cursor = index == window.files.cursor();
            let click = UsageMsg::FileClick(index);
            Some(framed(cells, index, cursor, tokens, click.clone(), click))
        });
    listed(
        header,
        rows,
        "disk_usage.largest",
        UsageMsg::FileScroll,
        UsageMsg::FileViewport,
    )
}

/// A row's name: a folder's, its own files' line, an unreadable folder marked.
fn name_of(row: &FolderRow) -> String {
    match row.node {
        None => t!("disk_usage.own_files").to_owned(),
        Some(_) if row.unreadable => format!("{}{}", row.name, t!("disk_usage.unreadable")),
        Some(_) => row.name.clone(),
    }
}

/// The share in percent and as a bar.
fn share<'a>(thousandths: u32, tokens: &'static Tokens) -> Element<'a, Message> {
    let filled = u16::try_from(thousandths.min(1000)).unwrap_or(1000);
    let mut bar = row![].width(BAR_WIDTH).height(8);
    if filled > 0 {
        bar = bar.push(
            container(Space::new())
                .width(Length::FillPortion(filled))
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(tokens.accent)),
                    ..container::Style::default()
                }),
        );
    }
    if filled < 1000 {
        bar = bar.push(Space::new().width(Length::FillPortion(1000 - filled)));
    }
    let percent = format!("{}.{} %", thousandths / 10, thousandths % 10);
    row![
        container(bar).style(move |_| theme::panel(tokens, false)),
        line(percent).width(Length::Fill).align_x(Horizontal::Right),
    ]
    .spacing(6)
    .width(SHARE_WIDTH)
    .align_y(Alignment::Center)
    .into()
}

/// A row on its stripe, the cursor's marked; a click and a double-click.
fn framed<'a>(
    cells: impl Into<Element<'a, Message>>,
    index: usize,
    cursor: bool,
    tokens: &'static Tokens,
    click: UsageMsg,
    open: UsageMsg,
) -> Element<'a, Message> {
    let framed = container(cells)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, true, false, theme::alternate(index)));
    // focus-ring: pointer only (a row of a list that is one stop)
    mouse_area(framed)
        .on_press(wrap(click))
        .on_double_click(wrap(open))
        .into()
}

/// The header over the rows, the rows as one stop of the ring; the wheel scrolls and the list
/// reports its height.
fn listed<'a>(
    header: impl Into<Element<'a, Message>>,
    rows: impl Iterator<Item = Element<'a, Message>>,
    label: &'static str,
    scroll: fn(isize) -> UsageMsg,
    viewport: fn(usize) -> UsageMsg,
) -> Element<'a, Message> {
    let list = container(Column::with_children(rows))
        .height(Length::Fill)
        .width(Length::Fill)
        .clip(true);
    let list = mouse_area(list)
        .on_scroll(move |delta| wrap(scroll(theme::wheel::rows(delta, theme::ROW_HEIGHT))));
    let list = sensor(list)
        .on_show(move |size| wrap(viewport(theme::rows_for(size.height))))
        .on_resize(move |size| wrap(viewport(theme::rows_for(size.height))));
    let list = focus::choice(list, label, None, None);
    column![header.into(), list]
        .spacing(4)
        .height(Length::Fill)
        .into()
}

/// Where the walk stands and what the folder shown holds.
fn status<'a>(window: &'a DiskUsageWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let total = &window.total;
    let mut text = t!(
        "disk_usage.total",
        size = size_of(window.choices.size_by, total),
        files = format::count(total.files),
        folders = format::count(total.dirs)
    );
    let (state, color) = match &window.stand {
        Stand::Starting | Stand::Running => {
            (t!("disk_usage.running").to_owned(), tokens.text_secondary)
        }
        Stand::Done => (String::new(), tokens.accent),
        Stand::Stopped => (t!("disk_usage.stopped").to_owned(), tokens.accent),
        Stand::Failed(error) => (error.clone(), theme::DANGER),
    };
    text.push_str(&state);
    if total.unreadable > 0 {
        text.push_str(&t!(
            "disk_usage.unreadable_count",
            count = format::count(total.unreadable)
        ));
    }
    if window.truncated {
        text.push_str(t!("disk_usage.truncated"));
    }
    fit::path_line(text, color, tokens)
}

/// Up, Show in panel and Feed to listbox at the left, Read again, Stop or Close at the right.
fn buttons(window: &DiskUsageWindow) -> Element<'_, Message> {
    let folders = window.tab == UsageTab::Folders;
    let close = if window.running() {
        layout::primary_icon_button(
            Icon::Stop,
            t!("disk_usage.stop"),
            Some(wrap(UsageMsg::Stop)),
        )
    } else {
        layout::primary_icon_button(
            Icon::Close,
            t!("disk_usage.close"),
            Some(wrap(UsageMsg::Close)),
        )
    };
    let reread = layout::secondary_button(t!("disk_usage.reread"), Some(wrap(UsageMsg::Reread)));
    let up = layout::secondary_button(t!("disk_usage.up"), folders.then(|| wrap(UsageMsg::Up)));
    let show = layout::secondary_button(
        t!("disk_usage.show_in_panel"),
        folders.then(|| wrap(UsageMsg::ShowInPanel)),
    );
    let feed = layout::secondary_button(
        t!("disk_usage.feed"),
        (!folders && !window.largest.is_empty()).then(|| wrap(UsageMsg::Feed)),
    );
    layout::button_row(Some(close), [reread], [up, show, feed])
}

fn tab<'a>(
    label: &'static str,
    tab: UsageTab,
    current: UsageTab,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let chosen = tab == current;
    let shown = container(line(label))
        .padding([4, 12])
        .style(move |_| theme::row(tokens, chosen, true, false));
    // focus-ring: pointer only (the tabs are one stop of the ring, the arrows choose)
    mouse_area(shown).on_press(wrap(UsageMsg::Tab(tab))).into()
}
