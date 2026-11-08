//! Where the dialog looks besides the folders (RC-100-93): "Search archives", off with a tooltip
//! where the panel is no local file system, and "Everything" where it is offered, off with a
//! tooltip while Everything does not run.

use iced::{Element, widget::row};

use super::wrap;
use crate::{
    app::{
        Message,
        search::{Everything, Flag, SearchDialog, SearchMsg},
    },
    i18n::t,
    theme::{Tokens, fit, layout},
};

/// The row of the two boxes on the first tab.
pub(super) fn sources<'a>(
    dialog: &'a SearchDialog,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let form = &dialog.form;
    let archives = boxed(
        t!("search.archives"),
        form.archives,
        Flag::Archives,
        (!dialog.local).then(|| t!("search.archives_local_only")),
        tokens,
    );
    let mut sources = row![archives].spacing(16);
    if dialog.everything != Everything::Hidden {
        let missing = dialog.everything == Everything::Missing;
        sources = sources.push(boxed(
            t!("search.everything"),
            form.everything,
            Flag::Everything,
            missing.then(|| t!("search.everything_missing")),
            tokens,
        ));
    }
    sources.into()
}

/// A box for `flag`; with a reason it is off, unticked, the reason in its tooltip.
fn boxed<'a>(
    label: &'static str,
    on: bool,
    flag: Flag,
    off: Option<&'static str>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let check = layout::check(label, on && off.is_none(), off.is_none(), move |on| {
        wrap(SearchMsg::Flag(flag, on))
    });
    match off {
        Some(reason) => fit::tip(check, reason.to_owned(), tokens),
        None => check,
    }
}
