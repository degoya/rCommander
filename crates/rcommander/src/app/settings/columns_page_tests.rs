//! The Columns tab over the app's `update()`, without a window (RC-100-90): what it reads, the
//! editor edited, what Save sends and what the engine keeps of it.

use rc_engine::{Align, ColumnSet, ColumnSetSpec, Command, Field};

use super::{ColumnsMsg, ColumnsPage, Editor, actions};
use crate::app::{
    Message,
    harness::Harness,
    resize::ResizeMsg,
    settings::{SettingsMessage, Tab},
};

fn set(id: i64, name: &str, masks: &[&str]) -> ColumnSet {
    let mut spec = ColumnSetSpec::of(&[Field::Name, Field::Size, Field::Created]);
    spec.masks = masks.iter().map(|mask| (*mask).to_owned()).collect();
    ColumnSet {
        id,
        name: name.into(),
        spec,
    }
}

fn columns(message: ColumnsMsg) -> Message {
    Message::Settings(SettingsMessage::Columns(message))
}

fn page(harness: &Harness) -> &ColumnsPage {
    &harness.app.settings.as_ref().expect("open").columns
}

#[test]
fn the_marked_set_stays_and_unsaved_edits_survive_a_read() {
    let mut page = ColumnsPage::default();
    page.sets_read(&[set(1, "A", &[]), set(2, "B", &[])]);
    assert_eq!(page.marked, Some(1), "the first is marked");
    page.marked = Some(2);
    page.editor = page.marked_set().map(Editor::of);
    if let Some(editor) = page.editor.as_mut() {
        editor.name = "Bee".into();
        editor.dirty = true;
    }
    page.sets_read(&[set(1, "A", &[]), set(2, "B", &[])]);
    assert_eq!(page.editor.as_ref().map(|e| e.name.as_str()), Some("Bee"));
    page.sets_read(&[set(1, "A", &[])]);
    assert_eq!(
        page.marked,
        Some(1),
        "a deleted set leaves the mark to the first"
    );
    assert_eq!(page.editor.as_ref().map(|e| e.id), Some(1));
}

#[test]
fn the_editor_reads_its_spec_back_and_refuses_a_width_that_is_no_number() {
    let editor = Editor::of(&set(1, "Photos", &[r"D:\Fotos\*", "sftp://*"]));
    assert_eq!(editor.masks, r"D:\Fotos\*;sftp://*");
    let spec = editor.spec().expect("reads");
    assert_eq!(spec, set(1, "Photos", &[r"D:\Fotos\*", "sftp://*"]).spec);
    let mut broken = editor.clone();
    broken.rows[1].width = "wide".into();
    assert_eq!(broken.spec(), Err("column_set_invalid"));
    let mut titled = editor;
    titled.rows[0].title = "  File ".into();
    titled.masks = " ; a\\* ;".into();
    let spec = titled.spec().expect("reads");
    assert_eq!(spec.columns[0].title.as_deref(), Some("File"));
    assert_eq!(spec.masks, ["a\\*"]);
}

#[tokio::test]
async fn edits_change_the_editor_and_save_sends_rename_and_update() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Columns)))
        .await;
    harness
        .send(Message::Resize(ResizeMsg::Sets(vec![set(
            7,
            "Photos",
            &[],
        )])))
        .await;
    assert_eq!(page(&harness).marked, Some(7));
    harness
        .send(columns(ColumnsMsg::Field(1, Field::Comment)))
        .await;
    harness
        .send(columns(ColumnsMsg::Align(1, Align::Right)))
        .await;
    harness.send(columns(ColumnsMsg::RowUp(1))).await;
    harness.send(columns(ColumnsMsg::AddRow)).await;
    harness.send(columns(ColumnsMsg::RowRemove(3))).await;
    harness
        .send(columns(ColumnsMsg::Name("Pictures".into())))
        .await;
    let editor = page(&harness).editor.as_ref().expect("an editor");
    let fields: Vec<Field> = editor.rows.iter().map(|row| row.field).collect();
    assert_eq!(fields, [Field::Comment, Field::Name, Field::Created]);
    assert_eq!(
        editor.rows[0].width,
        Field::Comment.default_width().to_string()
    );
    assert_eq!(editor.rows[0].align, Align::Right);
    assert!(editor.dirty);

    let commands = actions::save_commands(page(&harness))
        .expect("an editor")
        .expect("fits");
    assert!(matches!(
        &commands[..],
        [
            Command::RenameColumnSet { id: 7, .. },
            Command::UpdateColumnSet { id: 7, .. }
        ]
    ));
}

#[tokio::test]
async fn what_the_tab_sends_is_kept_by_the_engine() {
    let (_root, harness) = Harness::temp_with_db().await;
    let engine = harness.app.engine().clone();
    let create = Command::CreateColumnSet {
        name: "Photos".into(),
        spec: ColumnSetSpec::of(&[Field::Name, Field::Accessed]),
    };
    let saved = actions::run(engine.clone(), vec![create]).await;
    let id = saved.expect("saved").expect("an id");
    let refused = actions::run(
        engine.clone(),
        vec![Command::RenameColumnSet {
            id,
            name: " ".into(),
        }],
    )
    .await;
    assert_eq!(refused, Err("column_set_name"));
    let sets = engine.column_sets().await.expect("read");
    assert_eq!(sets[0].name, "Photos");
}
