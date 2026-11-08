//! Drawing "Select duplicate files" (RC-100-98): the two tabs, "leave one" with its criteria, and
//! the button row OK, Cancel, Apply. Every control is a stop of the focus ring.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{
        Column, column, container, mouse_area, pick_list, radio, row, scrollable, text_input,
    },
};
use rc_engine::search::{DatePick, Keep, SizePick};

use super::select_duplicates::{FilesRule, SelMsg, SelTab, SelectDuplicates, wrap};
use crate::{
    app::Message,
    i18n::t,
    theme::{
        self, Tokens,
        fit::line,
        focus,
        layout::{self},
    },
};

/// The card's height over the main window.
const CARD_HEIGHT: f32 = 520.0;

/// A criterion of "leave one" as its pick list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeepChoice(Keep);

const KEEPS: [KeepChoice; 6] = [
    KeepChoice(Keep::Newest),
    KeepChoice(Keep::Oldest),
    KeepChoice(Keep::Largest),
    KeepChoice(Keep::Smallest),
    KeepChoice(Keep::First),
    KeepChoice(Keep::Last),
];

impl fmt::Display for KeepChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Keep::Newest => t!("select_duplicates.keep_newest"),
            Keep::Oldest => t!("select_duplicates.keep_oldest"),
            Keep::Largest => t!("select_duplicates.keep_largest"),
            Keep::Smallest => t!("select_duplicates.keep_smallest"),
            Keep::First => t!("select_duplicates.keep_first"),
            Keep::Last => t!("select_duplicates.keep_last"),
        })
    }
}

/// The dialog's title.
pub fn title() -> &'static str {
    t!("select_duplicates.title")
}

/// The dialog's content: its window's whole, or the card's body.
pub fn body<'a>(dialog: &'a SelectDuplicates, tokens: &'static Tokens) -> Element<'a, Message> {
    let tabs = row![
        tab(
            t!("select_duplicates.tab_files"),
            SelTab::Files,
            dialog.tab,
            tokens
        ),
        tab(
            t!("select_duplicates.tab_folders"),
            SelTab::Folders,
            dialog.tab,
            tokens
        ),
    ]
    .spacing(6);
    let tabs = focus::pick(
        tabs,
        "select_duplicates.tabs",
        &[SelTab::Files, SelTab::Folders],
        Some(&dialog.tab),
        |tab| wrap(SelMsg::Tab(tab)),
    );
    let page = match dialog.tab {
        SelTab::Files => files(dialog),
        SelTab::Folders => folders(dialog, tokens),
    };
    let ok = layout::primary_button(t!("select_duplicates.ok"), Some(wrap(SelMsg::Ok)));
    let cancel =
        layout::secondary_button(t!("select_duplicates.cancel"), Some(wrap(SelMsg::Cancel)));
    let apply = layout::secondary_button(t!("select_duplicates.apply"), Some(wrap(SelMsg::Apply)));
    column![
        tabs,
        container(page).width(Length::Fill).height(Length::Fill),
        leave_one(dialog),
        layout::button_row(Some(ok), [cancel], [apply]),
    ]
    .spacing(12)
    .height(Length::Fill)
    .into()
}

/// The dialog as a card over the main window.
pub fn overlay<'a>(dialog: &'a SelectDuplicates, tokens: &'static Tokens) -> Element<'a, Message> {
    let content = container(body(dialog, tokens)).height(CARD_HEIGHT);
    Message::overlay(title(), content, layout::MEDIUM_DIALOG_WIDTH, tokens)
}

/// The tab "Files": the mask with its two rules, the four by date and the four by size, one
/// group of options.
fn files(dialog: &SelectDuplicates) -> Element<'_, Message> {
    let option = |rule: FilesRule| {
        radio(rule_label(rule), rule, Some(dialog.rule), |rule| {
            wrap(SelMsg::Rule(rule))
        })
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
    };
    let mask = text_input("*.*", &dialog.mask)
        .on_input(|mask| wrap(SelMsg::Mask(mask)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8]);
    let mask = layout::keyed(
        focus::Stop::field(mask, t!("select_duplicates.mask")),
        t!("select_duplicates.mask"),
    );
    let options = column![
        line(t!("select_duplicates.by_mask")),
        row![option(FilesRule::SelectMask), option(FilesRule::RemoveMask)].spacing(16),
        line(t!("select_duplicates.by_date")),
        row![
            option(FilesRule::Date(DatePick::AllOlder)),
            option(FilesRule::Date(DatePick::AllNewer)),
        ]
        .spacing(16),
        row![
            option(FilesRule::Date(DatePick::Oldest)),
            option(FilesRule::Date(DatePick::Newest)),
        ]
        .spacing(16),
        line(t!("select_duplicates.by_size")),
        row![
            option(FilesRule::Size(SizePick::AllLarger)),
            option(FilesRule::Size(SizePick::AllSmaller)),
        ]
        .spacing(16),
        row![
            option(FilesRule::Size(SizePick::Largest)),
            option(FilesRule::Size(SizePick::Smallest)),
        ]
        .spacing(16),
    ]
    .spacing(6);
    // The options are one stop of the ring; the arrows choose (RC-080-16).
    let options = layout::keyed(
        focus::Stop::pick(
            options,
            t!("select_duplicates.rules"),
            focus::moves(&FilesRule::ALL, Some(&dialog.rule), &|rule| {
                wrap(SelMsg::Rule(rule))
            }),
        ),
        t!("select_duplicates.rules"),
    );
    column![
        row![layout::caption(t!("select_duplicates.mask"), None), mask]
            .spacing(8)
            .align_y(Alignment::Center),
        options,
    ]
    .spacing(10)
    .into()
}

fn rule_label(rule: FilesRule) -> &'static str {
    match rule {
        FilesRule::SelectMask => t!("select_duplicates.select_mask"),
        FilesRule::RemoveMask => t!("select_duplicates.remove_mask"),
        FilesRule::Date(DatePick::AllOlder) => t!("select_duplicates.all_older"),
        FilesRule::Date(DatePick::AllNewer) => t!("select_duplicates.all_newer"),
        FilesRule::Date(DatePick::Oldest) => t!("select_duplicates.oldest"),
        FilesRule::Date(DatePick::Newest) => t!("select_duplicates.newest"),
        FilesRule::Size(SizePick::AllLarger) => t!("select_duplicates.all_larger"),
        FilesRule::Size(SizePick::AllSmaller) => t!("select_duplicates.all_smaller"),
        FilesRule::Size(SizePick::Largest) => t!("select_duplicates.largest"),
        FilesRule::Size(SizePick::Smallest) => t!("select_duplicates.smallest"),
    }
}

/// The tab "Select by folder": the folders to mark, the pattern with Select and Remove, the two
/// switches and All, None, Reverse.
fn folders<'a>(dialog: &'a SelectDuplicates, tokens: &'static Tokens) -> Element<'a, Message> {
    let boxes = dialog.folders.iter().map(|folder| {
        let shown = if folder.is_empty() {
            t!("select_duplicates.panel_folder")
        } else {
            folder.as_str()
        };
        let on = dialog.marked.contains(folder);
        let name = folder.clone();
        layout::check(shown, on, true, move |on| {
            wrap(SelMsg::Folder(name.clone(), on))
        })
    });
    let list = container(scrollable(
        Column::with_children(boxes).spacing(4).padding(6),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| theme::panel(tokens, true));
    let pattern = text_input("*", &dialog.pattern)
        .on_input(|pattern| wrap(SelMsg::Pattern(pattern)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8]);
    let pattern = layout::keyed(
        focus::Stop::field(pattern, t!("select_duplicates.pattern")),
        t!("select_duplicates.pattern"),
    );
    column![
        list,
        row![
            layout::caption(t!("select_duplicates.pattern"), None),
            pattern,
            layout::secondary_button(
                t!("select_duplicates.pattern_select"),
                Some(wrap(SelMsg::PatternMark(true)))
            ),
            layout::secondary_button(
                t!("select_duplicates.pattern_remove"),
                Some(wrap(SelMsg::PatternMark(false)))
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            layout::check(
                t!("select_duplicates.subfolders"),
                dialog.subfolders,
                true,
                |on| wrap(SelMsg::Subfolders(on))
            ),
            layout::check(
                t!("select_duplicates.parent_only"),
                dialog.parent_only,
                true,
                |on| wrap(SelMsg::ParentOnly(on))
            ),
        ]
        .spacing(16),
        row![
            layout::secondary_button(t!("select_duplicates.all"), Some(wrap(SelMsg::All))),
            layout::secondary_button(t!("select_duplicates.none"), Some(wrap(SelMsg::Nothing))),
            layout::secondary_button(t!("select_duplicates.reverse"), Some(wrap(SelMsg::Reverse))),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

/// "Leave at least one file in each group unselected" and its three criteria.
fn leave_one(dialog: &SelectDuplicates) -> Element<'_, Message> {
    let criterion = |at: usize, label: &'static str| {
        let chosen = KeepChoice(dialog.keep[at]);
        let choose = move |choice: KeepChoice| wrap(SelMsg::Keep(at, choice.0));
        let list = pick_list(KEEPS, Some(chosen), choose)
            .text_size(theme::TEXT_SIZE)
            .width(130);
        row![
            layout::caption(label, None),
            focus::pick(list, label, &KEEPS, Some(&chosen), choose),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    column![
        layout::check(
            t!("select_duplicates.leave_one"),
            dialog.leave_one,
            true,
            |on| wrap(SelMsg::LeaveOne(on))
        ),
        row![
            criterion(0, t!("select_duplicates.keep_1")),
            criterion(1, t!("select_duplicates.keep_2")),
            criterion(2, t!("select_duplicates.keep_3")),
        ]
        .spacing(16),
    ]
    .spacing(6)
    .into()
}

fn tab<'a>(
    label: &'static str,
    tab: SelTab,
    current: SelTab,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let chosen = tab == current;
    let shown = container(line(label))
        .padding([4, 12])
        .style(move |_| theme::row(tokens, chosen, true, false));
    // focus-ring: pointer only (the tabs are one stop of the ring, the arrows choose)
    mouse_area(shown).on_press(wrap(SelMsg::Tab(tab))).into()
}
