//! What the remote edits draw: a question's text and buttons, Done's question, the list
//! "Server files being edited" and the status bar's "N server files open".

use iced::{
    Element, Length,
    widget::{Column, Space, checkbox, column, container, mouse_area, scrollable, text},
};
use rc_engine::{RemoteEditInfo, RemoteEditQuestion, RemoteEditState, UploadChoice};

use super::{RemoteEditMsg, RemoteEditUi, wrap};
use crate::{
    app::{App, Message},
    i18n::{self, t},
    theme::{
        self, Tokens,
        fit::{self, label, label_or_glyph},
        focus, layout,
    },
};

/// The rows the list shows before it scrolls.
const LIST_ROWS: f32 = 10.0;

impl App {
    /// The title of the open question about an edit.
    pub(in crate::app) fn remote_edit_title(&self) -> Option<&'static str> {
        let ui = &self.quit.remote;
        if ui.discard.is_some() {
            return Some(t!("remote_edit.discard.title"));
        }
        Some(match ui.asking()?.question? {
            RemoteEditQuestion::Upload => t!("remote_edit.ask.upload_title"),
            RemoteEditQuestion::ServerChanged | RemoteEditQuestion::ServerGone => {
                t!("remote_edit.ask.conflict_title")
            }
            RemoteEditQuestion::InPlace => t!("remote_edit.ask.in_place_title"),
            RemoteEditQuestion::Kept => t!("remote_edit.ask.kept_title"),
        })
    }

    /// The question over the window while one is open (`dialog_windows.rs`).
    pub(in crate::app) fn remote_edit_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let title = label(self.remote_edit_title()?).font(theme::UI_FONT_STRONG);
        Some(layout::dialog_card(
            Message::dialog_head(title),
            self.remote_edit_body()?,
            layout::SMALL_DIALOG_WIDTH,
            tokens,
        ))
    }

    /// The open question's text and buttons: on its card, or filling a window of its own.
    pub(in crate::app) fn remote_edit_body(&self) -> Option<Element<'_, Message>> {
        let ui = &self.quit.remote;
        if let Some(id) = ui.discard {
            let name = ui
                .edits
                .iter()
                .find(|edit| edit.id == id)
                .map_or("", |edit| edit.name.as_str());
            let buttons = layout::button_row(
                Some(layout::primary_button(
                    t!("remote_edit.discard.keep"),
                    Some(wrap(RemoteEditMsg::Discard(false))),
                )),
                [layout::secondary_button(
                    t!("remote_edit.discard.discard"),
                    Some(wrap(RemoteEditMsg::Discard(true))),
                )],
                [],
            );
            let text = t!("remote_edit.discard.text", name = name);
            return Some(column![fit::prose(text), buttons].spacing(10).into());
        }
        let edit = ui.asking()?;
        Some(question(ui, edit, edit.question?))
    }

    /// The list "Server files being edited" over the window while it is open.
    pub(in crate::app) fn remote_edit_list_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let ui = &self.quit.remote;
        let row = ui.list?;
        let title = label(t!("remote_edit.list.title")).font(theme::UI_FONT_STRONG);
        let rows: Element<'_, Message> = if ui.edits.is_empty() {
            label(t!("remote_edit.list.empty")).into()
        } else {
            let lines = ui
                .edits
                .iter()
                .enumerate()
                .map(|(index, edit)| list_row(index, index == row, edit, tokens));
            let list = scrollable(Column::with_children(lines))
                .height(Length::Fixed(theme::ROW_HEIGHT * LIST_ROWS));
            focus::choice(
                list,
                t!("remote_edit.list.title"),
                row.checked_sub(1).map(|up| wrap(RemoteEditMsg::Select(up))),
                Some(row + 1)
                    .filter(|down| *down < ui.edits.len())
                    .map(|down| wrap(RemoteEditMsg::Select(down))),
            )
        };
        let chosen = ui.cursor();
        let upload = chosen
            .filter(|edit| edit.state.unsaved() && edit.state != RemoteEditState::Uploading)
            .map(|edit| wrap(RemoteEditMsg::Upload(edit.id, UploadChoice::Upload)));
        let done = chosen.map(|edit| wrap(RemoteEditMsg::Done(edit.id)));
        let buttons = layout::button_row(
            Some(layout::primary_button(
                t!("remote_edit.list.close"),
                Some(wrap(RemoteEditMsg::CloseList)),
            )),
            [
                layout::secondary_button(t!("remote_edit.list.upload_now"), upload),
                layout::secondary_button(t!("remote_edit.list.done"), done),
            ],
            [],
        );
        Some(layout::dialog_card(
            Message::dialog_head(title),
            column![rows, buttons].spacing(10),
            layout::MEDIUM_DIALOG_WIDTH,
            tokens,
        ))
    }
}

/// The question `asked` about `edit`: its text, its buttons - the safe one the default where
/// the server's file would be replaced -, after a save the box.
fn question<'a>(
    ui: &'a RemoteEditUi,
    edit: &'a RemoteEditInfo,
    asked: RemoteEditQuestion,
) -> Element<'a, Message> {
    let (name, server) = (edit.name.as_str(), edit.server.as_str());
    let id = edit.id;
    let upload = |choice| Some(wrap(RemoteEditMsg::Upload(id, choice)));
    let skip = Some(wrap(RemoteEditMsg::Skip(id)));
    let (text, buttons) = match asked {
        RemoteEditQuestion::Upload => (
            t!("remote_edit.ask.upload", name = name, server = server),
            layout::button_row(
                Some(layout::primary_button(
                    t!("remote_edit.ask.upload_button"),
                    upload(UploadChoice::Upload),
                )),
                [layout::secondary_button(
                    t!("remote_edit.ask.not_now"),
                    skip,
                )],
                [],
            ),
        ),
        RemoteEditQuestion::ServerChanged => (
            t!(
                "remote_edit.ask.server_changed",
                name = name,
                server = server
            ),
            layout::button_row(
                Some(layout::primary_button(t!("remote_edit.ask.not_now"), skip)),
                [
                    layout::secondary_button(
                        t!("remote_edit.ask.overwrite"),
                        upload(UploadChoice::Overwrite),
                    ),
                    layout::secondary_button(
                        t!("remote_edit.ask.as_copy"),
                        upload(UploadChoice::AsCopy),
                    ),
                ],
                [],
            ),
        ),
        RemoteEditQuestion::ServerGone => (
            t!("remote_edit.ask.server_gone", name = name, server = server),
            layout::button_row(
                Some(layout::primary_button(t!("remote_edit.ask.not_now"), skip)),
                [layout::secondary_button(
                    t!("remote_edit.ask.upload_anyway"),
                    upload(UploadChoice::Overwrite),
                )],
                [],
            ),
        ),
        RemoteEditQuestion::InPlace => (
            t!("remote_edit.ask.in_place", name = name, server = server),
            layout::button_row(
                Some(layout::primary_button(t!("remote_edit.ask.cancel"), skip)),
                [layout::secondary_button(
                    t!("remote_edit.ask.in_place_button"),
                    upload(UploadChoice::InPlace),
                )],
                [],
            ),
        ),
        RemoteEditQuestion::Kept => (
            t!("remote_edit.ask.kept", name = name, server = server),
            layout::button_row(
                Some(layout::primary_button(
                    t!("remote_edit.ask.upload_button"),
                    upload(UploadChoice::Upload),
                )),
                [
                    layout::secondary_button(
                        t!("remote_edit.ask.discard"),
                        Some(wrap(RemoteEditMsg::Close(id))),
                    ),
                    layout::secondary_button(t!("remote_edit.ask.later"), skip),
                ],
                [],
            ),
        ),
    };
    let mut body = column![fit::prose(text)].spacing(10);
    if asked == RemoteEditQuestion::Upload {
        let always = checkbox(ui.always)
            .label(t!("remote_edit.ask.always"))
            .on_toggle(|on| wrap(RemoteEditMsg::Always(on)))
            .size(14)
            .text_size(theme::TEXT_SIZE)
            .font(theme::UI_FONT);
        body = body.push(focus::toggle(
            always,
            t!("remote_edit.ask.always"),
            Some(wrap(RemoteEditMsg::Always(!ui.always))),
        ));
    }
    body.push(buttons).into()
}

/// One row of the list: the name, the server and folder, the state; the cursor's marked.
fn list_row<'a>(
    index: usize,
    cursor: bool,
    edit: &'a RemoteEditInfo,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let folder = edit
        .path
        .path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();
    let cells = vec![
        fit::path_cell(edit.name.clone(), 3, false, tokens),
        fit::path_cell(format!("{}: {folder}", edit.server), 4, true, tokens),
        fit::cell(state_text(&edit.state), 3, true, tokens),
    ];
    let line = container(fit::table_row(false, tokens, cells))
        .padding([0, 6])
        .width(Length::Fill)
        .style(move |_| theme::striped_row(tokens, cursor, true, false, theme::alternate(index)));
    // focus-ring: pointer only (a row of the list, one choice stop; the arrows move the cursor)
    mouse_area(line)
        .on_press(wrap(RemoteEditMsg::Select(index)))
        .into()
}

/// What the list says of an edit's state.
fn state_text(state: &RemoteEditState) -> String {
    match state {
        RemoteEditState::Unchanged => t!("remote_edit.state.unchanged").to_owned(),
        RemoteEditState::Changed => t!("remote_edit.state.changed").to_owned(),
        RemoteEditState::Uploading => t!("remote_edit.state.uploading").to_owned(),
        RemoteEditState::Uploaded { at } => {
            let time = chrono::DateTime::<chrono::Local>::from(*at).format("%H:%M");
            t!("remote_edit.state.uploaded", time = time.to_string())
        }
        RemoteEditState::Conflict => t!("remote_edit.state.conflict").to_owned(),
        RemoteEditState::Failed { code } => {
            t!("remote_edit.state.failed", error = i18n::error_text(code))
        }
    }
}

/// "3 server files open" while copies are edited, a click opens their list; nothing otherwise.
pub fn status<'a>(app: &App, tokens: &'static Tokens) -> Element<'a, Message> {
    let count = app.quit.remote.edits.len();
    if count == 0 {
        return Space::new().into();
    }
    let face = label_or_glyph(t!("status.remote_edits", count = count))
        .size(theme::STATUS_SIZE)
        .color(tokens.text_secondary)
        .wrapping(text::Wrapping::None);
    // focus-ring: pointer only (the status bar is no dialog; the palette's row opens the list)
    mouse_area(face)
        .on_press(wrap(RemoteEditMsg::OpenList))
        .into()
}
