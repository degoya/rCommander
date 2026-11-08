//! The question for a program's parameters (RC-100-86), as Total Commander asks when the
//! parameters of a button or a Start menu entry begin with `?`: the engine announces
//! `UserCommandAsk` instead of starting the program, and a small dialog shows what the command is
//! called and its parameters in a field to change. Run starts it again with them - `usercmd.run`,
//! `start_menu.run` or `file.run_association` (RC-100-91) with `params` -, Cancel or Escape drops
//! it and nothing starts. A card over
//! the window, or a window of its own while the small questions have theirs
//! (`dialog_windows/questions.rs`, `Question::AskParams`).

use iced::{
    Element,
    widget::{column, text_input},
};
use rc_engine::{AskRequest, AskSource, Command};

use super::{UcMsg, focus};
use crate::{
    app::{App, Effect, Message, escape::escape, field_walk},
    i18n::t,
    theme::{self, Tokens, fit, icon::Icon, layout},
};

/// The parameters' field: the keyboard starts here.
pub const ASK_FIELD: &str = "rcommander.user_commands.ask";

/// The open question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub request: AskRequest,
    /// The parameters as typed; the stored ones without the `?` at first.
    pub text: String,
}

/// What the question's field and buttons say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskMsg {
    Input(String),
    Run,
    Cancel,
}

/// The command that runs the asked program with `params`, for the panel it was asked for.
pub fn command_of(request: &AskRequest, params: String) -> Command {
    let panel = Some(request.panel);
    match &request.source {
        AskSource::Own { name } => Command::RunUserCommand {
            name: name.clone(),
            panel,
            params: Some(params),
        },
        AskSource::Start { id } => Command::RunStartItem {
            id: *id,
            panel,
            params: Some(params),
        },
        // A file's internal association (RC-100-91), Enter's `open` among them.
        AskSource::Association { path, verb } => Command::RunAssociation {
            panel,
            path: Some(path.clone()),
            verb: verb.clone(),
            params: Some(params),
        },
        // A button of a bar runs again with the files dropped on it (RC-100-87).
        AskSource::Button { bar, button, files } => Command::RunButton {
            bar: *bar,
            button: *button,
            panel,
            params: Some(params),
            files: files.clone(),
        },
    }
}

fn wrap(message: AskMsg) -> Message {
    Message::UserCommands(UcMsg::Ask(message))
}

impl App {
    /// `UserCommandAsk`: the question opens - a newer one takes the place of one still open -,
    /// the keyboard in its field.
    pub(in crate::app) fn ask_params(&mut self, request: AskRequest) -> Vec<Effect> {
        let text = request.params.clone();
        self.user_commands.ask = Some(Ask { request, text });
        vec![focus(ASK_FIELD)]
    }

    /// A message of the question; Run and Cancel close it at once, so a second click sends
    /// nothing.
    pub(in crate::app) fn on_ask(&mut self, message: AskMsg) -> Vec<Effect> {
        match message {
            AskMsg::Input(typed) => {
                if let Some(ask) = self.user_commands.ask.as_mut() {
                    ask.text = typed;
                }
                Vec::new()
            }
            AskMsg::Run => match self.user_commands.ask.take() {
                Some(ask) => {
                    let panel = ask.request.panel;
                    vec![Effect::Dispatch(
                        panel,
                        vec![command_of(&ask.request, ask.text)],
                    )]
                }
                None => Vec::new(),
            },
            AskMsg::Cancel => {
                self.user_commands.ask = None;
                Vec::new()
            }
        }
    }

    /// While the question is open it has every key and click: Tab walks its controls, Enter
    /// runs, Escape cancels; `None` for any other message, or while it is closed.
    pub(in crate::app) fn ask_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        self.user_commands.ask.as_ref()?;
        match message {
            Message::Key(chord) if field_walk::tab(chord).is_some() => field_walk::walk_key(chord),
            Message::Key(chord) | Message::KeyCaptured(chord) if *chord == escape() => {
                Some(self.on_ask(AskMsg::Cancel))
            }
            // Enter with no control that took it; the field's own Enter submits.
            Message::Key(chord) if chord.key == "Enter" && chord.is_plain() => {
                Some(self.on_ask(AskMsg::Run))
            }
            Message::Key(_) | Message::KeyCaptured(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// The question over the window while it is open.
    pub(in crate::app) fn ask_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let title = fit::label(t!("user_commands.ask.title")).font(theme::UI_FONT_STRONG);
        Some(layout::dialog_card(
            Message::dialog_head(title),
            self.ask_body()?,
            layout::SMALL_DIALOG_WIDTH,
            tokens,
        ))
    }

    /// The question's text, field and buttons while it is open: on its card, or filling a window
    /// of its own.
    pub(in crate::app) fn ask_body(&self) -> Option<Element<'_, Message>> {
        let ask = self.user_commands.ask.as_ref()?;
        let field = text_input("", &ask.text)
            .id(ASK_FIELD)
            .font(theme::UI_FONT)
            .size(theme::TEXT_SIZE)
            .padding([5, 8])
            .on_input(|typed| wrap(AskMsg::Input(typed)))
            .on_submit(wrap(AskMsg::Run));
        let buttons = layout::button_row(
            Some(layout::primary_icon_button(
                Icon::Confirm,
                t!("user_commands.ask.run"),
                Some(wrap(AskMsg::Run)),
            )),
            [layout::secondary_button(
                t!("user_commands.ask.cancel"),
                Some(wrap(AskMsg::Cancel)),
            )],
            [],
        );
        let text = t!("user_commands.ask.text", title = ask.request.title.as_str());
        Some(column![fit::prose(text), field, buttons].spacing(10).into())
    }
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
