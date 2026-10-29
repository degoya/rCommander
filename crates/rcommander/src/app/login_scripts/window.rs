//! The editor in the window: opened from the connection manager ("Login scripts..."), drawn as
//! a layer over it, or in a modeless window of its own beside it (RC-080-15); Escape, Close or
//! the window's X closes it. Its one button row: Delete at the left, Save (the default) and Close
//! at the right (RC-050-13). Its [`Action`]s run off the window's thread and come back as its
//! own messages; a script an agent changes meanwhile is read again with the list.

use iced::{
    Element, Length, Task,
    widget::{column, container, text},
};
use rc_engine::{
    Command, Engine, Outcome, login_script,
    settings::{WARN_SECRET_PROMPT_KEY, stored_bool},
};

use super::{Action, Editor, Message as EditorMsg};
use crate::{
    app::{
        App, Effect, Message,
        escape::{enter, escape},
        field_walk,
    },
    i18n::t,
    theme::{self, Tokens, icon::Icon, layout},
};

impl App {
    /// Opens the editor (once) and reads the scripts into it.
    pub(in crate::app) fn open_login_scripts(&mut self) -> Vec<Effect> {
        if self.login_scripts.is_some() {
            return Vec::new();
        }
        let (editor, action) = Editor::open();
        self.login_scripts = Some(editor);
        let mut effects = self.login_script_action(action);
        effects.push(Effect::Task(self.login_scripts_warn_task()));
        effects
    }

    /// Reads `login_scripts.warn_secret_prompt` into the open editor: when it opens and when the
    /// setting changes (RC-050-13).
    pub(in crate::app) fn login_scripts_warn_task(&self) -> Task<Message> {
        let (true, Some(db)) = (self.login_scripts.is_some(), self.db.clone()) else {
            return Task::none();
        };
        Task::perform(
            async move { stored_bool(&db, WARN_SECRET_PROMPT_KEY).await },
            |on| Message::LoginScripts(EditorMsg::WarnPrompt(on)),
        )
    }

    /// A message of the editor.
    pub(in crate::app) fn on_login_scripts(&mut self, message: EditorMsg) -> Vec<Effect> {
        let Some(editor) = self.login_scripts.as_mut() else {
            return Vec::new();
        };
        let action = editor.apply(message);
        self.login_script_action(action)
    }

    /// A login script changed in the engine: the open editor's list is read again.
    pub(in crate::app) fn login_scripts_changed(&self) -> Vec<Effect> {
        if self.login_scripts.is_none() {
            return Vec::new();
        }
        self.login_script_action(Action::Dispatch(Command::ListLoginScripts))
    }

    /// While the editor is open the keys are its fields' (Escape closes it) and a click is its.
    pub(in crate::app) fn login_scripts_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        self.login_scripts.as_ref()?;
        match message {
            Message::Key(chord) | Message::KeyCaptured(chord) if *chord == escape() => {
                self.login_scripts = None;
                Some(Vec::new())
            }
            // Tab a field did not take walks the focus ring; Enter no field or button took is
            // Save, the default button (RC-080-16).
            Message::Key(chord) if field_walk::tab(chord).is_some() => field_walk::walk_key(chord),
            Message::Key(chord) if *chord == enter() => {
                Some(self.on_login_scripts(EditorMsg::Save))
            }
            Message::Key(_) | Message::KeyCaptured(_) | Message::Press => Some(Vec::new()),
            Message::CloseLoginScripts => {
                self.login_scripts = None;
                Some(Vec::new())
            }
            _ => None,
        }
    }

    /// The editor's layer of the window's dialog stack, while it is open.
    pub(in crate::app) fn login_scripts_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let body = self.login_scripts_body()?;
        // A large dialog: the head, the editor in the room left, Close at the bottom right.
        let title = text(t!("login_scripts.title"))
            .size(18)
            .font(theme::UI_FONT);
        Some(theme::large_dialog(
            Message::dialog_head(title),
            body,
            tokens,
        ))
    }

    /// The editor and its Close, while it is open: in the large dialog, or filling a window of
    /// its own (RC-080-15, `dialog_windows.rs`).
    pub(in crate::app) fn login_scripts_body(&self) -> Option<Element<'_, Message>> {
        let editor = self.login_scripts.as_ref()?;
        let tokens = theme::tokens(self.dark);
        let deletable = editor.draft.id.is_some();
        let editor = container(editor.view(tokens).map(Message::LoginScripts)).height(Length::Fill);
        // Save is the default (Enter); Delete, which acts on the open script, stands apart at the
        // left (RC-050-13).
        let buttons = layout::button_row(
            Some(layout::primary_icon_button(
                Icon::Save,
                t!("login_scripts.save"),
                Some(Message::LoginScripts(EditorMsg::Save)),
            )),
            [layout::secondary_icon_button(
                Icon::Close,
                t!("connections.close"),
                Some(Message::CloseLoginScripts),
            )],
            [layout::secondary_icon_button(
                Icon::Delete,
                t!("login_scripts.delete"),
                deletable.then_some(Message::LoginScripts(EditorMsg::Delete)),
            )],
        );
        Some(column![editor, buttons].spacing(12).into())
    }

    fn login_script_action(&self, action: Action) -> Vec<Effect> {
        let task = match action {
            Action::None => return Vec::new(),
            Action::Dispatch(command) => Task::future(dispatch(self.engine.clone(), command))
                .and_then(|message| Task::done(Message::LoginScripts(message))),
            Action::Rehearse(script, sample) => Task::perform(
                async move {
                    login_script::rehearse(&script, &sample)
                        .await
                        .map_err(|error| error.code())
                },
                |result| Message::LoginScripts(EditorMsg::Rehearsed(result)),
            ),
        };
        vec![Effect::Task(task)]
    }
}

/// Runs an editor's command; its outcome as the editor's message. Every command of the editor
/// answers a list, a script or a deletion; another answer leaves the editor as it is.
async fn dispatch(engine: Engine, command: Command) -> Option<EditorMsg> {
    let listing = matches!(command, Command::ListLoginScripts);
    let deleting = matches!(command, Command::DeleteLoginScript { .. });
    Some(match engine.dispatch(command).await {
        Ok(Outcome::LoginScripts { scripts }) => EditorMsg::Listed(Ok(scripts)),
        Ok(Outcome::LoginScript { script }) => EditorMsg::Saved(Ok(script)),
        Ok(Outcome::LoginScriptDeleted { .. }) => EditorMsg::Deleted(Ok(())),
        Ok(_) => return None,
        Err(error) if listing => EditorMsg::Listed(Err(error.code())),
        Err(error) if deleting => EditorMsg::Deleted(Err(error.code())),
        Err(error) => EditorMsg::Saved(Err(error.code())),
    })
}
