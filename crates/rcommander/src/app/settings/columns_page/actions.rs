//! What the Columns tab's buttons and fields do (RC-100-90): an edit changes the editor, a button
//! sends its `column_sets.*` command off the window's thread. A new set is named after the
//! sets there are ("Column set 2"), a copy after its original ("Photos (copy)").

use iced::Task;
use rc_engine::{
    Align, ColumnSetSpec, ColumnSetsOutcome, Command, Engine, Field, Outcome, columns::MAX_COLUMNS,
};

use super::{ColumnsMsg, ColumnsPage, Editor, Row, page_message};
use crate::{
    app::Effect,
    i18n::{error_text, t},
};

/// The columns of a new set: Total Commander's full view.
const NEW_FIELDS: [Field; 4] = [Field::Name, Field::Ext, Field::Size, Field::Modified];

/// Runs `commands` one after the other; the set the last saved, or the code of the first refusal.
pub(super) async fn run(
    engine: Engine,
    commands: Vec<Command>,
) -> Result<Option<i64>, &'static str> {
    let mut saved = None;
    for command in commands {
        match engine.dispatch(command).await {
            Ok(Outcome::ColumnSets(outcome)) => {
                if let ColumnSetsOutcome::Saved { id } = *outcome {
                    saved = Some(id);
                }
            }
            Ok(_) => return Err("column_set_invalid"),
            Err(error) => return Err(error.code()),
        }
    }
    Ok(saved)
}

/// Sends `commands`; the answer comes back as [`ColumnsMsg::Answered`].
fn send(page: &mut ColumnsPage, engine: &Engine, commands: Vec<Command>) -> Vec<Effect> {
    page.busy = true;
    page.note = None;
    let engine = engine.clone();
    vec![Effect::Task(Task::perform(
        run(engine, commands),
        |result| page_message(ColumnsMsg::Answered(result)),
    ))]
}

/// `name`, numbered from 2 while another set has it (case does not count).
fn free_name(page: &ColumnsPage, name: &str) -> String {
    let taken = |candidate: &str| {
        page.sets
            .iter()
            .any(|set| set.name.eq_ignore_ascii_case(candidate))
    };
    if !taken(name) {
        return name.to_owned();
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| name.to_owned())
}

/// The editor's row `at`, edited by `change`; the editor is dirty then.
fn edit_row(page: &mut ColumnsPage, at: usize, change: impl FnOnce(&mut Row)) {
    if let Some(editor) = page.editor.as_mut()
        && let Some(row) = editor.rows.get_mut(at)
    {
        change(row);
        editor.dirty = true;
    }
}

/// The editor edited by `change`; dirty then.
fn edit(page: &mut ColumnsPage, change: impl FnOnce(&mut Editor)) {
    if let Some(editor) = page.editor.as_mut() {
        change(editor);
        editor.dirty = true;
    }
}

/// The marked set moved by `step` places.
fn moved(page: &mut ColumnsPage, engine: &Engine, step: isize) -> Vec<Effect> {
    let Some((id, at)) = page.marked.and_then(|id| Some((id, page.index_of(id)?))) else {
        return Vec::new();
    };
    let Some(position) = at
        .checked_add_signed(step)
        .filter(|to| *to < page.sets.len())
    else {
        return Vec::new();
    };
    send(page, engine, vec![Command::MoveColumnSet { id, position }])
}

/// What `message` does to the tab.
pub(super) fn apply(page: &mut ColumnsPage, engine: &Engine, message: ColumnsMsg) -> Vec<Effect> {
    match message {
        ColumnsMsg::Choose(id) => {
            page.marked = Some(id);
            page.editor = page.marked_set().map(Editor::of);
        }
        ColumnsMsg::New => {
            let name = free_name(page, t!("settings.columns.new_name"));
            let spec = ColumnSetSpec::of(&NEW_FIELDS);
            return send(page, engine, vec![Command::CreateColumnSet { name, spec }]);
        }
        ColumnsMsg::Copy => {
            let Some(set) = page.marked_set() else {
                return Vec::new();
            };
            let name = t!("settings.columns.copy_name", name = set.name);
            let (name, spec) = (free_name(page, &name), set.spec.clone());
            return send(page, engine, vec![Command::CreateColumnSet { name, spec }]);
        }
        ColumnsMsg::Up => return moved(page, engine, -1),
        ColumnsMsg::Down => return moved(page, engine, 1),
        ColumnsMsg::Delete => {
            let Some(id) = page.marked else {
                return Vec::new();
            };
            return send(page, engine, vec![Command::DeleteColumnSet { id }]);
        }
        ColumnsMsg::Name(name) => edit(page, |editor| editor.name = name),
        ColumnsMsg::Masks(masks) => edit(page, |editor| editor.masks = masks),
        ColumnsMsg::Field(at, field) => edit_row(page, at, |row| {
            row.field = field;
            row.width = field.default_width().to_string();
            row.align = field.default_align();
        }),
        ColumnsMsg::Title(at, title) => edit_row(page, at, |row| row.title = title),
        ColumnsMsg::Width(at, width) => edit_row(page, at, |row| row.width = width),
        ColumnsMsg::Align(at, align) => edit_row(page, at, |row| row.align = align),
        ColumnsMsg::RowUp(at) => edit(page, |editor| {
            if at > 0 && at < editor.rows.len() {
                editor.rows.swap(at - 1, at);
            }
        }),
        ColumnsMsg::RowDown(at) => edit(page, |editor| {
            if at + 1 < editor.rows.len() {
                editor.rows.swap(at, at + 1);
            }
        }),
        ColumnsMsg::RowRemove(at) => edit(page, |editor| {
            if editor.rows.len() > 1 && at < editor.rows.len() {
                editor.rows.remove(at);
            }
        }),
        ColumnsMsg::AddRow => edit(page, |editor| {
            if editor.rows.len() < MAX_COLUMNS {
                editor.rows.push(Row {
                    field: Field::Comment,
                    title: String::new(),
                    width: Field::Comment.default_width().to_string(),
                    align: Align::Left,
                });
            }
        }),
        ColumnsMsg::Save => return save(page, engine),
        ColumnsMsg::Answered(result) => {
            page.busy = false;
            match result {
                Ok(saved) => {
                    // A new set is marked; its editor follows once the sets are read again.
                    if let Some(id) = saved {
                        page.marked = Some(id);
                        page.editor = page.marked_set().map(Editor::of);
                    }
                    if let Some(editor) = page.editor.as_mut() {
                        editor.dirty = false;
                    }
                    page.note = Some((t!("settings.columns.saved").to_owned(), false));
                }
                Err(code) => page.note = Some((error_text(code), true)),
            }
        }
    }
    Vec::new()
}

/// What Save sends for the editor: the name when it changed, then the columns and the masks;
/// the code of why not when a width is no number. `None` without an editor.
pub(super) fn save_commands(page: &ColumnsPage) -> Option<Result<Vec<Command>, &'static str>> {
    let editor = page.editor.as_ref()?;
    let spec = match editor.spec() {
        Ok(spec) => spec,
        Err(code) => return Some(Err(code)),
    };
    let id = editor.id;
    let mut commands = Vec::new();
    let renamed = page
        .marked_set()
        .is_some_and(|set| set.name != editor.name.trim());
    if renamed {
        commands.push(Command::RenameColumnSet {
            id,
            name: editor.name.clone(),
        });
    }
    commands.push(Command::UpdateColumnSet { id, spec });
    Some(Ok(commands))
}

/// Save: [`save_commands`] sent, or the note why not.
fn save(page: &mut ColumnsPage, engine: &Engine) -> Vec<Effect> {
    match save_commands(page) {
        Some(Ok(commands)) => send(page, engine, commands),
        Some(Err(code)) => {
            page.note = Some((error_text(code), true));
            Vec::new()
        }
        None => Vec::new(),
    }
}
