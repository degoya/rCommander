//! The right side: the form of the workspace at the cursor - its name, its key with "Add key...",
//! its theme of each mode, the active panel and the split -, then each panel (`panel.rs`); a hint
//! while there is no workspace to show. Each control is a stop of the focus ring.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, checkbox, column, container, pick_list, row, scrollable},
};
use rc_engine::PanelId;

use super::{field, panel, wrap};
use crate::{
    app::{
        Message,
        palette::themes,
        workspaces::{
            Shown, WorkspaceWindow, WsMsg,
            form::{Form, FormMsg, SPLITS},
            known,
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, layout},
};

/// The width of the labels in front of the controls.
pub(super) const LABEL_WIDTH: f32 = 130.0;
/// The form's name field.
pub const FORM_NAME_FIELD: &str = "rcommander.workspaces.form_name";

/// A value and its text in a list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Labeled<T>(pub T, pub String);

impl<T> fmt::Display for Labeled<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.1)
    }
}

/// A change of the form, as the window's message.
pub(super) fn form_message(message: FormMsg) -> Message {
    wrap(WsMsg::Form(message))
}

pub(super) fn side<'a>(
    window: &'a WorkspaceWindow,
    shown: &Shown,
    key: Option<String>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(form) = &window.form else {
        return container(fit::hint(t!("workspaces.choose"), tokens))
            .padding(8)
            .into();
    };
    let name = field("", &form.name)
        .id(FORM_NAME_FIELD)
        .on_input(|typed| form_message(FormMsg::Name(typed)))
        .on_submit(wrap(WsMsg::Save));
    let key = row![
        fit::label(key.unwrap_or_else(|| t!("workspaces.key_none").to_owned())).width(Length::Fill),
        layout::secondary_button(t!("workspaces.add_key"), Some(wrap(WsMsg::AddKey))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let rows = Column::new()
        .spacing(8)
        .push(labeled(t!("workspaces.field.name"), name))
        .push(labeled(t!("workspaces.field.key"), key))
        .push(labeled(
            t!("workspaces.field.theme_light"),
            theme_choice(form, false),
        ))
        .push(labeled(
            t!("workspaces.field.theme_dark"),
            theme_choice(form, true),
        ))
        .push(labeled(t!("workspaces.field.active"), active_choice(form)))
        .push(labeled(t!("workspaces.field.split"), split_choice(form)))
        .push(labeled(t!("workspaces.field.start"), start_toggle(form)))
        .push(panel::section(form, PanelId::Left, shown, tokens))
        .push(panel::section(form, PanelId::Right, shown, tokens));
    column![
        fit::label(t!("workspaces.edit_title")).font(theme::UI_FONT_STRONG),
        scrollable(rows.padding([0, 12])).height(Length::Fill),
    ]
    .spacing(10)
    .into()
}

/// `control` behind its label.
pub(super) fn labeled<'a>(
    label: &'static str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        fit::label(label).width(LABEL_WIDTH),
        container(control).width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// A choice among `options` with `current` chosen, a stop of the ring named `label`: the arrows
/// choose the option before and after, as a closed combo box does.
pub(super) fn choice<'a, T>(
    options: Vec<Labeled<T>>,
    current: Option<&T>,
    on_select: impl Fn(T) -> Message + 'a,
    label: &str,
    width: impl Into<Length>,
) -> Element<'a, Message>
where
    T: Clone + PartialEq + 'a,
{
    let selected = current
        .and_then(|current| options.iter().find(|option| option.0 == *current))
        .cloned();
    let pick = move |picked: Labeled<T>| on_select(picked.0);
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(width);
    focus::Stop::pick(list, label, moves).into()
}

/// The theme of the light or the `dark` mode: the window's, then each theme of the mode; one the
/// window no longer knows shows as the window's, which saving then stores.
fn theme_choice(form: &Form, dark: bool) -> Element<'_, Message> {
    let known: Vec<themes::ThemeRow> = themes::known()
        .into_iter()
        .filter(|row| row.mode.is_dark() == dark)
        .collect();
    let current = form
        .theme(dark)
        .filter(|id| known.iter().any(|row| row.id == *id))
        .map(str::to_owned);
    let default = Labeled(None, t!("workspaces.theme_default").to_owned());
    let options = std::iter::once(default)
        .chain(known.into_iter().map(|row| Labeled(Some(row.id), row.name)))
        .collect();
    let label = if dark {
        "workspaces.theme_dark"
    } else {
        "workspaces.theme_light"
    };
    choice(
        options,
        Some(&current),
        move |id| form_message(FormMsg::Theme { dark, id }),
        label,
        260,
    )
}

/// Whether rCommander calls this workspace up when it starts (`workspaces.on_start`).
fn start_toggle(form: &Form) -> Element<'_, Message> {
    let on = known::start() == Some(form.id);
    let label = t!("workspaces.start_up");
    let check = checkbox(on)
        .label(label)
        .on_toggle(|on| wrap(WsMsg::StartUp(on)))
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT);
    focus::toggle(check, label, Some(wrap(WsMsg::StartUp(!on))))
}

/// The panel that has the keyboard after the workspace is applied.
fn active_choice(form: &Form) -> Element<'_, Message> {
    let options = vec![
        Labeled(PanelId::Left, t!("workspaces.panel.left").to_owned()),
        Labeled(PanelId::Right, t!("workspaces.panel.right").to_owned()),
    ];
    choice(
        options,
        Some(&form.spec.active),
        |panel| form_message(FormMsg::Active(panel)),
        "workspaces.active",
        260,
    )
}

/// The left panel's share of the width: as it is, or a tenth from 10 to 90 %; a stored value
/// between those is offered too.
fn split_choice(form: &Form) -> Element<'_, Message> {
    let current = form.spec.split_permille;
    let mut splits: Vec<u16> = SPLITS.to_vec();
    if let Some(stored) = current.filter(|stored| !splits.contains(stored)) {
        splits.push(stored);
        splits.sort_unstable();
    }
    let keep = Labeled(None, t!("workspaces.split_keep").to_owned());
    let options = std::iter::once(keep)
        .chain(splits.into_iter().map(|permille| {
            let percent = format!("{:.1}", f32::from(permille) / 10.0);
            let percent = percent.trim_end_matches(".0").to_owned();
            Labeled(
                Some(permille),
                t!("workspaces.split_percent", percent = percent),
            )
        }))
        .collect();
    choice(
        options,
        Some(&current),
        |split| form_message(FormMsg::Split(split)),
        "workspaces.split",
        260,
    )
}
