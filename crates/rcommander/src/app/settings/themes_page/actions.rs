//! What the Themes tab's buttons do (RC-100-50): New, Duplicate, Rename and Delete through the
//! engine's `theme.save` and `theme.delete`, Import and Export through a file, the question when
//! an import's name is taken - and, once the engine answered, the themes read again.

use iced::Task;
use rc_engine::{
    Command, Engine, Outcome,
    settings::themes::{ImportedTheme, Theme, ThemeAccent, ThemeDraft, ThemesOutcome, unique_name},
};

use super::{
    Answer, Conflict, ConflictChoice, ThemesMsg, ThemesPage, editor::Editor, file, page_message,
};
use crate::{
    app::{App, Effect},
    i18n::{error_text, t},
    theme,
};

/// The draft `theme` is saved as; a new theme goes without its id.
pub(super) fn draft_of(theme: &Theme, new: bool) -> ThemeDraft {
    ThemeDraft {
        id: (!new).then(|| theme.id.clone()),
        name: theme.name.clone(),
        dark: theme.dark,
        accent: theme.accent.as_text().to_owned(),
        tokens: theme.tokens.clone(),
    }
}

/// The name a theme is shown under: a built-in one's in the interface's language.
pub(super) fn shown_name(theme: &Theme) -> &str {
    theme::builtin::name(&theme.id)
        .filter(|_| theme.builtin)
        .unwrap_or(&theme.name)
}

/// The names of the own themes, which a new name must not repeat.
fn own_names(page: &ThemesPage) -> Vec<String> {
    page.state
        .iter()
        .flat_map(|state| state.themes.iter())
        .filter(|theme| !theme.builtin)
        .map(|theme| theme.name.clone())
        .collect()
}

/// `name`, numbered where an own theme has it.
fn free_name(page: &ThemesPage, name: &str) -> String {
    let names = own_names(page);
    unique_name(name, names.iter().map(String::as_str))
}

/// Runs `command`; the themes after it, or the code of why not.
pub(super) async fn run(
    engine: Engine,
    command: Command,
) -> Result<Box<ThemesOutcome>, &'static str> {
    match engine.dispatch(command).await {
        Ok(Outcome::Themes(outcome)) => Ok(outcome),
        Ok(_) => Err("theme_invalid"),
        Err(error) => Err(error.code()),
    }
}

/// Sends `command` to the engine; its answer comes back as [`ThemesMsg::Answered`] with `note`.
pub(super) fn send(
    engine: &Engine,
    command: Command,
    note: String,
    edit: Option<Theme>,
    close_editor: bool,
) -> Effect {
    let engine = engine.clone();
    Effect::Task(Task::perform(run(engine, command), move |result| {
        page_message(ThemesMsg::Answered(Box::new(Answer {
            result,
            note,
            edit: edit.map(Box::new),
            close_editor,
        })))
    }))
}

/// A new own theme of `name` and `dark` with these colours, saved and opened in the editor.
fn create(engine: &Engine, page: &mut ThemesPage, theme: Theme, note: String) -> Vec<Effect> {
    page.busy = true;
    let command = Command::SaveTheme {
        theme: draft_of(&theme, true),
    };
    vec![send(engine, command, note, Some(theme), false)]
}

/// The theme a file brought, saved under `id` (`None`: as a new one) and `name`.
fn save_imported(
    engine: &Engine,
    page: &mut ThemesPage,
    imported: ImportedTheme,
    id: Option<String>,
    name: String,
) -> Vec<Effect> {
    let note = if imported.dropped.is_empty() {
        t!("settings.themes.imported", name = name)
    } else {
        t!(
            "settings.themes.imported_dropped",
            name = name,
            tokens = imported.dropped.join(", ")
        )
    };
    let theme = ThemeDraft {
        id,
        name,
        dark: imported.dark,
        accent: imported.accent.as_text().to_owned(),
        tokens: imported.tokens,
    };
    page.busy = true;
    vec![send(
        engine,
        Command::SaveTheme { theme },
        note,
        None,
        false,
    )]
}

impl App {
    /// A button, a field or a file of the Themes tab.
    pub(super) fn themes_action(&mut self, message: ThemesMsg) -> Vec<Effect> {
        let engine = self.engine.clone();
        let dark_now = self.dark;
        let Some(page) = self.settings.as_mut().map(|dialog| &mut dialog.themes) else {
            return Vec::new();
        };
        let mut restyle = false;
        let effects = match message {
            ThemesMsg::Choose(id) => {
                page.selected = Some(id);
                page.renaming = None;
                Vec::new()
            }
            ThemesMsg::New => {
                let dark = page.marked().map_or(dark_now, |theme| theme.dark);
                let theme = Theme {
                    id: String::new(),
                    name: free_name(page, t!("settings.themes.new_name")),
                    dark,
                    accent: ThemeAccent::System,
                    tokens: Default::default(),
                    builtin: false,
                };
                create(
                    &engine,
                    page,
                    theme,
                    t!("settings.themes.created").to_owned(),
                )
            }
            ThemesMsg::Duplicate => match page.marked().cloned() {
                Some(source) => {
                    let (accent, tokens) = file::own_colours(&source);
                    let copy = t!("settings.themes.copy_name", name = shown_name(&source));
                    let theme = Theme {
                        id: String::new(),
                        name: free_name(page, &copy),
                        dark: source.dark,
                        accent,
                        tokens,
                        builtin: false,
                    };
                    create(
                        &engine,
                        page,
                        theme,
                        t!("settings.themes.created").to_owned(),
                    )
                }
                None => Vec::new(),
            },
            ThemesMsg::Edit => {
                if let Some(theme) = page.marked().filter(|theme| !theme.builtin).cloned() {
                    page.editor = Some(Editor::open(theme));
                    restyle = true;
                }
                Vec::new()
            }
            ThemesMsg::Delete => {
                let id = page
                    .marked()
                    .filter(|theme| !theme.builtin)
                    .map(|theme| theme.id.clone())
                    .filter(|id| !page.chosen(id));
                match id {
                    Some(id) => {
                        page.busy = true;
                        let note = t!("settings.themes.deleted").to_owned();
                        vec![send(
                            &engine,
                            Command::DeleteTheme { id },
                            note,
                            None,
                            false,
                        )]
                    }
                    None => Vec::new(),
                }
            }
            ThemesMsg::Rename => {
                page.renaming = page
                    .marked()
                    .filter(|theme| !theme.builtin)
                    .map(|theme| theme.name.clone());
                Vec::new()
            }
            ThemesMsg::RenameText(text) => {
                if page.renaming.is_some() {
                    page.renaming = Some(text);
                }
                Vec::new()
            }
            ThemesMsg::RenameCancel => {
                page.renaming = None;
                Vec::new()
            }
            ThemesMsg::RenameDone => {
                let name = page.renaming.take();
                match (page.marked().cloned(), name) {
                    (Some(theme), Some(name)) if !theme.builtin => {
                        let theme = Theme { name, ..theme };
                        let command = Command::SaveTheme {
                            theme: draft_of(&theme, false),
                        };
                        page.busy = true;
                        let note = t!("settings.themes.renamed").to_owned();
                        vec![send(&engine, command, note, None, false)]
                    }
                    _ => Vec::new(),
                }
            }
            ThemesMsg::File(text) => {
                page.file = text;
                Vec::new()
            }
            ThemesMsg::Import => match file::path_of(&page.file, false) {
                Some(path) => {
                    page.busy = true;
                    vec![Effect::Task(Task::perform(file::read(path), |read| {
                        page_message(ThemesMsg::Imported(read.map(Box::new)))
                    }))]
                }
                None => {
                    page.note = Some((t!("settings.themes.no_file").to_owned(), true));
                    Vec::new()
                }
            },
            ThemesMsg::Export => {
                let bytes = page.marked().map(file::exported);
                match (bytes, file::path_of(&page.file, true)) {
                    (Some(bytes), Some(path)) => {
                        page.busy = true;
                        vec![Effect::Task(Task::perform(
                            file::write(path, bytes),
                            |done| page_message(ThemesMsg::Exported(done)),
                        ))]
                    }
                    _ => {
                        page.note = Some((t!("settings.themes.no_file").to_owned(), true));
                        Vec::new()
                    }
                }
            }
            ThemesMsg::Imported(Ok(imported)) => {
                page.busy = false;
                let existing = page
                    .state
                    .iter()
                    .flat_map(|state| state.themes.iter())
                    .find(|theme| {
                        !theme.builtin && theme.name.to_lowercase() == imported.name.to_lowercase()
                    })
                    .cloned();
                match existing {
                    Some(existing) => {
                        page.conflict = Some(Box::new(Conflict {
                            imported: *imported,
                            existing,
                        }));
                        Vec::new()
                    }
                    None => {
                        let name = imported.name.clone();
                        save_imported(&engine, page, *imported, None, name)
                    }
                }
            }
            ThemesMsg::Imported(Err(code)) | ThemesMsg::Exported(Err(code)) => {
                page.busy = false;
                page.note = Some((error_text(code), true));
                Vec::new()
            }
            ThemesMsg::Exported(Ok(path)) => {
                page.busy = false;
                page.note = Some((t!("settings.themes.exported", path = path), false));
                Vec::new()
            }
            ThemesMsg::Conflict(choice) => match (page.conflict.take(), choice) {
                (Some(conflict), ConflictChoice::Replace) => {
                    let Conflict { imported, existing } = *conflict;
                    save_imported(&engine, page, imported, Some(existing.id), existing.name)
                }
                (Some(conflict), ConflictChoice::Rename) => {
                    let name = free_name(page, &conflict.imported.name);
                    save_imported(&engine, page, conflict.imported, None, name)
                }
                _ => Vec::new(),
            },
            ThemesMsg::Read(_) | ThemesMsg::Answered(_) | ThemesMsg::Editor(_) => Vec::new(),
        };
        if restyle {
            self.restyle_terminals();
        }
        effects
    }

    /// The engine answered a save or a delete: the note, the editor opened on a theme made or
    /// closed after it saved, and the themes read again.
    pub(super) fn themes_answered(&mut self, answer: Answer) -> Vec<Effect> {
        if let Some(page) = self.settings.as_mut().map(|dialog| &mut dialog.themes) {
            page.busy = false;
            match answer.result {
                Ok(outcome) => {
                    page.note = Some((answer.note, false));
                    if let Some(id) = outcome.saved {
                        page.selected = Some(id.clone());
                        if let Some(theme) = answer.edit {
                            page.editor = Some(Editor::open(Theme { id, ..*theme }));
                        }
                    }
                    if answer.close_editor {
                        page.editor = None;
                    }
                }
                Err(code) => page.note = Some((error_text(code), true)),
            }
        }
        self.restyle_terminals();
        vec![Effect::Task(self.themes_task())]
    }
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
