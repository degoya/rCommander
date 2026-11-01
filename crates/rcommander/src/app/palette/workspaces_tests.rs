use rc_engine::{Keymap, Overrides};

use super::*;
use crate::app::palette::{Palette, entries, reach};

fn row(id: WorkspaceId, name: &str, active: bool) -> WorkspaceRow {
    WorkspaceRow {
        id,
        name: name.to_owned(),
        active,
    }
}

/// Two workspaces, the second active.
fn listed() -> Vec<WorkspaceRow> {
    vec![row(3, "Servers", false), row(7, "Local & web", true)]
}

fn named<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|entry| entry.name == name)
}

/// A key's name holds the workspace's id; another name holds none.
#[test]
fn a_key_name_holds_the_id() {
    assert_eq!(key_name(12), "workspace:12");
    assert_eq!(key_id("workspace:12"), Some(12));
    for wrong in ["workspace:", "workspace:x", "theme:dark:nord-dark", "copy"] {
        assert_eq!(key_id(wrong), None, "{wrong}");
    }
}

/// One row per workspace, named by it, running `workspaces.apply`, the active one marked; a
/// workspace no longer listed has no row.
#[test]
fn the_rows_follow_the_list_of_workspaces() {
    let mut entries = entries(&Keymap::standard(), &reach::standard(), false);
    let before = entries.len();
    add_rows(&mut entries, &listed());
    assert_eq!(entries.len(), before + 2);
    let servers = named(&entries, "workspace:3").expect("a row");
    assert_eq!(servers.label, row_label("Servers"));
    assert_eq!(
        servers.action,
        Action::Run(Box::new(Command::ApplyWorkspace { id: 3 }))
    );
    assert!(!servers.current);
    assert!(named(&entries, "workspace:7").is_some_and(|entry| entry.current));
    let labels: Vec<String> = entries
        .iter()
        .map(|entry| entry.label.to_lowercase())
        .collect();
    assert!(labels.is_sorted(), "sorted by label");

    let mut entries = super::super::entries(&Keymap::standard(), &reach::standard(), false);
    add_rows(&mut entries, &listed()[..1]);
    assert!(named(&entries, "workspace:7").is_none());
}

/// The search finds a row by the word "workspace" and by the workspace's name.
#[test]
fn the_search_finds_a_workspace_by_word_and_name() {
    let mut entries = entries(&Keymap::standard(), &reach::standard(), false);
    add_rows(&mut entries, &listed());
    let mut palette = Palette::new(entries);
    palette.set_query("workspace servers".into());
    let shown: Vec<&str> = palette
        .shown
        .iter()
        .filter_map(|index| palette.entry(*index))
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(shown, ["workspace:3"]);
}

/// A workspace key is named by its workspace, and by its id once the workspace is gone, which
/// marks it missing.
#[test]
fn a_key_is_named_by_its_workspace() {
    let rows = listed();
    assert_eq!(
        label_in("workspace:3", &rows).as_deref(),
        Some(row_label("Servers").as_str())
    );
    assert_eq!(
        label_in("workspace:99", &rows).as_deref(),
        Some(row_label("99").as_str())
    );
    assert!(missing("workspace:99", &rows));
    assert!(!missing("workspace:3", &rows));
    assert!(!missing("copy", &rows), "no workspace key");
    assert_eq!(label_in("copy", &rows), None);
}

/// The menu lists the workspaces first, the first nine with a digit as the access key, a name's
/// own `&` kept as one, then a separator; without workspaces it lists none.
#[test]
fn the_menu_lists_the_workspaces_first() {
    let keymap = Keymap::standard();
    assert!(menu_items_of(&[], &keymap).is_empty());
    let items = menu_items_of(&listed(), &keymap);
    assert_eq!(
        items,
        [
            BarItem::Entry {
                id: "workspace:3",
                text: "&1 Servers".to_owned(),
            },
            BarItem::Entry {
                id: "workspace:7",
                text: "&2 Local && web".to_owned(),
            },
            BarItem::Separator,
        ]
    );
    let many: Vec<WorkspaceRow> = (1..=10)
        .map(|id| row(id, &format!("W{id}"), false))
        .collect();
    let items = menu_items_of(&many, &keymap);
    assert!(
        matches!(&items[9], BarItem::Entry { text, .. } if text == "W10"),
        "the tenth has no digit"
    );
    assert!(
        menu_items("file", &keymap).is_empty(),
        "only the Workspaces menu"
    );
}

/// A workspace with a key of its own shows it after a tab, as every entry of the bar does; its
/// palette row is the key's row.
#[test]
fn a_workspace_key_shows_in_the_menu_and_the_palette() {
    let overrides = Overrides::parse("Ctrl+Alt+1=workspace:3").expect("a deviation");
    let keymap = Keymap::with_overrides(&overrides).0;
    let items = menu_items_of(&listed(), &keymap);
    let key = crate::keymap::label(&"Ctrl+Alt+1".parse().expect("a chord"));
    assert_eq!(
        items[0],
        BarItem::Entry {
            id: "workspace:3",
            text: format!("&1 Servers\t{key}"),
        }
    );
    let mut entries = entries(&keymap, &reach::standard(), false);
    let before = entries.len();
    add_rows(&mut entries, &listed());
    assert_eq!(
        entries.len(),
        before + 1,
        "the key's row stands for the first"
    );
    let keyed = named(&entries, "workspace:3").expect("the key's row");
    assert_eq!(keyed.label, row_label("Servers"));
    assert!(matches!(keyed.action, Action::Press(_)));
}

/// An id is made once per workspace, however often the menu is built.
#[test]
fn an_id_is_made_once() {
    let first = interned("workspace:41".to_owned());
    let again = interned("workspace:41".to_owned());
    assert!(std::ptr::eq(first, again));
}
