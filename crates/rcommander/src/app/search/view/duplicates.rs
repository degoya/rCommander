//! "Find duplicate files" on the second tab (RC-100-98): the switch, and beside it the three
//! criteria - same name, size, contents -, which only count while it is on.

use iced::{Element, widget::row};

use super::wrap;
use crate::{
    app::{
        Message,
        search::{Flag, Form, SearchMsg},
    },
    i18n::t,
    theme::layout,
};

/// The switch and its criteria in one row.
pub(super) fn duplicates<'a>(form: &Form) -> Element<'a, Message> {
    let on = form.duplicates;
    let criterion = |label: &'static str, ticked: bool, flag: Flag| {
        layout::check(label, ticked, on, move |ticked| {
            wrap(SearchMsg::Flag(flag, ticked))
        })
    };
    row![
        layout::check(t!("search.duplicates"), on, true, |ticked| {
            wrap(SearchMsg::Flag(Flag::Duplicates, ticked))
        }),
        criterion(t!("search.same_name"), form.same_name, Flag::SameName),
        criterion(t!("search.same_size"), form.same_size, Flag::SameSize),
        criterion(
            t!("search.same_contents"),
            form.same_contents,
            Flag::SameContents
        ),
    ]
    .spacing(16)
    .into()
}
