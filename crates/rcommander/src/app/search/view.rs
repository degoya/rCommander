//! Drawing the find-files dialog over the dimmed window, and the badge a flat panel shows in
//! its path bar.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, mouse_area, pick_list, radio, row, text_input},
};
use rc_engine::{FlatSource, FlatView, MAX_FLAT_ENTRIES, search::KindFilter};

use super::{Attribute, Field, Flag, Form, Run, SearchDialog, SearchMsg, Tab, Tri};
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

mod hits;

use hits::hit_list;

const LABEL_WIDTH: f32 = 130.0;

impl fmt::Display for Tri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Any => t!("search.attr_any"),
            Self::Set => t!("search.attr_set"),
            Self::Unset => t!("search.attr_unset"),
        })
    }
}

fn wrap(message: SearchMsg) -> Message {
    Message::Search(message)
}

/// The dialog over the window.
pub fn overlay<'a>(dialog: &'a SearchDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    // A large dialog: the hit list takes the height the form leaves.
    let head = Message::titled_head(t!("search.title"));
    theme::large_dialog(head, body(dialog, tokens), tokens)
}

/// The tabs, the form, the hits and the buttons: the card's content over the window, the whole
/// of its own window (RC-080-15).
pub fn body<'a>(dialog: &'a SearchDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let tabs = row![
        tab(t!("search.tab_general"), Tab::General, dialog.tab, tokens),
        tab(t!("search.tab_advanced"), Tab::Advanced, dialog.tab, tokens),
    ]
    .spacing(6);
    // The tabs are one stop of the ring; the arrows choose (RC-080-16).
    let tabs = focus::pick(
        tabs,
        "search.tabs",
        &[Tab::General, Tab::Advanced],
        Some(&dialog.tab),
        |tab| wrap(SearchMsg::Tab(tab)),
    );
    let page = match dialog.tab {
        Tab::General => general(dialog),
        Tab::Advanced => advanced(&dialog.form),
    };
    column![
        tabs,
        container(page).width(Length::Fill),
        status(dialog, tokens),
        hit_list(dialog, tokens),
        buttons(dialog),
    ]
    .spacing(10)
    .into()
}

/// What a flat panel says beside its path: a branch view or search hits, still reading, cut.
pub fn flat_badge<'a>(flat: &FlatView, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut label = match flat.source {
        FlatSource::Branch => t!("flat.branch").to_owned(),
        FlatSource::Selection => t!("flat.selection").to_owned(),
        FlatSource::Paths => t!("flat.paths").to_owned(),
    };
    if flat.loading {
        label.push_str(t!("flat.loading"));
    }
    if flat.truncated {
        label.push_str(&t!(
            "flat.truncated",
            limit = format::count(MAX_FLAT_ENTRIES as u64)
        ));
    }
    if flat.skipped > 0 {
        label.push_str(&t!("flat.skipped", count = format::count(flat.skipped)));
    }
    line(label).color(tokens.accent).into()
}

fn general(dialog: &SearchDialog) -> Element<'_, Message> {
    let form = &dialog.form;
    let recall = |mask: String| wrap(SearchMsg::Recall(mask));
    let recent = pick_list(dialog.history.as_slice(), None::<String>, recall)
        .placeholder(t!("search.recent"))
        .text_size(theme::TEXT_SIZE)
        .width(150);
    let recent = focus::pick(recent, t!("search.recent"), &dialog.history, None, recall);
    column![
        labelled(
            t!("search.name"),
            row![
                field(&form.name, Field::Name, "*.*", t!("search.name")),
                recent
            ]
            .spacing(6)
        ),
        indented(
            row![
                flag(t!("search.name_regex"), form.name_regex, Flag::NameRegex),
                flag(t!("search.case"), form.name_case, Flag::NameCase),
            ]
            .spacing(16)
        ),
        labelled(
            t!("search.start"),
            field(&form.start, Field::Start, "", t!("search.start"))
        ),
        labelled(
            t!("search.depth"),
            field(
                &form.depth,
                Field::Depth,
                t!("search.depth_all"),
                t!("search.depth")
            )
        ),
        labelled(
            t!("search.text"),
            field(
                &form.text,
                Field::Text,
                t!("search.text_none"),
                t!("search.text")
            )
        ),
        indented(
            row![
                flag(t!("search.text_case"), form.text_case, Flag::TextCase),
                flag(t!("search.whole_words"), form.text_words, Flag::TextWords),
                flag(t!("search.text_regex"), form.text_regex, Flag::TextRegex),
                flag(
                    t!("search.not_containing"),
                    form.not_containing,
                    Flag::NotContaining
                ),
                flag(t!("search.skip_binary"), form.skip_binary, Flag::SkipBinary),
            ]
            .spacing(16)
        ),
        indented(
            row![
                flag(t!("search.utf8"), form.utf8, Flag::Utf8),
                flag(t!("search.utf16_le"), form.utf16_le, Flag::Utf16Le),
                flag(t!("search.utf16_be"), form.utf16_be, Flag::Utf16Be),
                flag(
                    t!("search.windows1252"),
                    form.windows1252,
                    Flag::Windows1252
                ),
                line(t!("search.encoding_auto")),
            ]
            .spacing(16)
        ),
    ]
    .spacing(8)
    .into()
}

fn advanced(form: &Form) -> Element<'_, Message> {
    let kind = |label: &'static str, value: KindFilter| {
        radio(label, value, Some(form.kind), |kind| {
            wrap(SearchMsg::Kind(kind))
        })
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
    };
    let attribute = |label: &'static str, value: Tri, attribute: Attribute| {
        let choose = move |tri| wrap(SearchMsg::Attribute(attribute, tri));
        let list = pick_list(Tri::ALL, Some(value), choose)
            .text_size(theme::TEXT_SIZE)
            .width(110);
        row![
            layout::caption(label, None),
            layout::keyed(
                focus::Stop::pick(list, label, focus::moves(&Tri::ALL, Some(&value), &choose)),
                label
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    column![
        labelled(
            t!("search.min_size"),
            row![
                field(&form.min_size, Field::MinSize, "", t!("search.min_size")),
                layout::caption(t!("search.max_size"), None),
                field(&form.max_size, Field::MaxSize, "", t!("search.max_size")),
                line(t!("search.size_units")),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ),
        labelled(
            t!("search.days"),
            field(
                &form.days,
                Field::Days,
                t!("search.days_any"),
                t!("search.days")
            )
        ),
        labelled(
            t!("search.after"),
            row![
                field(
                    &form.after,
                    Field::After,
                    t!("search.date_format"),
                    t!("search.after")
                ),
                layout::caption(t!("search.before"), None),
                field(
                    &form.before,
                    Field::Before,
                    t!("search.date_format"),
                    t!("search.before")
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ),
        labelled(
            t!("search.kind"),
            layout::keyed(
                focus::Stop::pick(
                    row![
                        kind(t!("search.kind_any"), KindFilter::Any),
                        kind(t!("search.kind_files"), KindFilter::Files),
                        kind(t!("search.kind_dirs"), KindFilter::Dirs),
                    ]
                    .spacing(16),
                    t!("search.kind"),
                    focus::moves(
                        &[KindFilter::Any, KindFilter::Files, KindFilter::Dirs],
                        Some(&form.kind),
                        &|kind| wrap(SearchMsg::Kind(kind))
                    )
                ),
                t!("search.kind")
            )
        ),
        labelled(
            t!("search.attributes"),
            row![
                attribute(t!("search.hidden"), form.hidden, Attribute::Hidden),
                attribute(t!("search.readonly"), form.readonly, Attribute::Readonly),
            ]
            .spacing(16)
        ),
        indented(
            row![
                attribute(t!("search.system"), form.system, Attribute::System),
                attribute(t!("search.archive"), form.archive, Attribute::Archive),
            ]
            .spacing(16)
        ),
        indented(
            row![
                flag(
                    t!("search.follow_links"),
                    form.follow_links,
                    Flag::FollowLinks
                ),
                flag(t!("search.skip_hidden"), form.skip_hidden, Flag::SkipHidden),
            ]
            .spacing(16)
        ),
    ]
    .spacing(8)
    .into()
}

/// One row under the hits: Go to and Feed to a panel at the left, Start (or Stop) and Close at
/// the right.
fn buttons(dialog: &SearchDialog) -> Element<'_, Message> {
    let found = !dialog.results.is_empty();
    let start = if dialog.running() {
        layout::primary_icon_button(Icon::Stop, t!("search.stop"), Some(wrap(SearchMsg::Stop)))
    } else {
        layout::primary_icon_button(
            Icon::Search,
            t!("search.start_button"),
            Some(wrap(SearchMsg::Start)),
        )
    };
    let on_hits = |message: SearchMsg| found.then(|| wrap(message));
    layout::button_row(
        Some(start),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("search.close"),
            Some(wrap(SearchMsg::Close)),
        )],
        [
            layout::secondary_button(t!("search.go_to"), on_hits(SearchMsg::GoTo)),
            layout::secondary_button(t!("search.feed"), on_hits(SearchMsg::Feed)),
        ],
    )
}

/// Where the search stands, or the field that does not read.
fn status<'a>(dialog: &'a SearchDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let danger = theme::DANGER;
    let hits = format::count(dialog.results.len() as u64);
    let (message, color) = match (&dialog.invalid, &dialog.run) {
        (Some(field), _) => (invalid(*field).to_owned(), danger),
        (None, Run::Idle) => (t!("search.idle").to_owned(), tokens.text_secondary),
        (None, Run::Running { dir, stats }) => (
            t!(
                "search.running",
                hits = hits,
                entries = format::count(stats.entries),
                dir = dir
                    .as_ref()
                    .map(|dir| dir.path.display().to_string())
                    .unwrap_or_default()
            ),
            tokens.text_secondary,
        ),
        (None, Run::Finished { stats, cancelled }) => {
            let mut done = if *cancelled {
                t!("search.cancelled", hits = hits)
            } else {
                t!("search.finished", hits = hits)
            };
            if stats.skipped > 0 {
                done.push_str(&t!("search.skipped", count = format::count(stats.skipped)));
            }
            if dialog.results.truncated {
                done.push_str(&t!(
                    "search.truncated",
                    limit = format::count(MAX_FLAT_ENTRIES as u64)
                ));
            }
            (done, tokens.accent)
        }
        (None, Run::Failed(error)) => (error.clone(), danger),
    };
    // An error wraps; the progress (with the folder being read) stays one line, shortened in
    // the middle.
    let failed = matches!(
        (&dialog.invalid, &dialog.run),
        (Some(_), _) | (None, Run::Failed(_))
    );
    if failed {
        fit::prose(message).color(color).into()
    } else {
        fit::path_line(message, color, tokens)
    }
}

fn invalid(field: Field) -> &'static str {
    match field {
        Field::Depth => t!("search.invalid_depth"),
        Field::MinSize | Field::MaxSize => t!("search.invalid_size"),
        Field::Days => t!("search.invalid_days"),
        Field::After | Field::Before => t!("search.invalid_date"),
        Field::Name | Field::Start | Field::Text => t!("search.invalid"),
    }
}

fn tab<'a>(
    label: &'static str,
    tab: Tab,
    current: Tab,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let chosen = tab == current;
    let shown = container(line(label))
        .padding([4, 12])
        .style(move |_| theme::row(tokens, chosen, true, false));
    mouse_area(shown).on_press(wrap(SearchMsg::Tab(tab))).into()
}

/// `content` after its caption `label`; the control that answers to the caption's access key
/// takes it itself ([`field`], `layout::keyed`).
fn labelled<'a>(
    label: &'static str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(layout::caption(label, None)).width(LABEL_WIDTH),
        content.into()
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn indented<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![Space::new().width(LABEL_WIDTH), content.into()]
        .spacing(8)
        .into()
}

/// A field answering to the access key of its caption `label`.
fn field<'a>(
    value: &'a str,
    field: Field,
    placeholder: &'a str,
    label: &'static str,
) -> Element<'a, Message> {
    let input = text_input(placeholder, value)
        .on_input(move |text| wrap(SearchMsg::Text(field, text)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8]);
    layout::keyed(focus::Stop::field(input, label), label)
}

fn flag<'a>(label: &'static str, on: bool, flag: Flag) -> Element<'a, Message> {
    layout::check(label, on, true, move |on| wrap(SearchMsg::Flag(flag, on)))
}
