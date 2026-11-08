use std::sync::Arc;

use rc_vfs::{Attributes, Entry, EntryKind, VfsPath};

use super::*;
use crate::{SortOrder, comments::Descriptions};

fn file(name: &str) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::File,
        size: 0,
        modified: None,
        created: None,
        accessed: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

#[test]
fn comments_are_read_from_descript_ion_and_looked_up_without_regard_to_case() {
    let descriptions =
        Descriptions::parse("a.txt first\r\n\"b c.txt\" second\r\nA.TXT again\r\nnonsense\r\n");
    let comments = FolderComments::of(&descriptions);
    assert_eq!(
        comments.get("A.txt"),
        Some("first"),
        "the first line counts"
    );
    assert_eq!(comments.get("b c.txt"), Some("second"));
    assert_eq!(comments.get("missing"), None);
    assert_eq!(comments.len(), 3, "{comments:?}");
    assert!(!comments.is_empty());
}

#[test]
fn the_comments_stay_in_the_same_folder_only() {
    let columns = PanelColumns {
        manual: true,
        rule: Some(3),
        before_comments: Some(ListMode::Brief),
        comments: Some(Arc::new(FolderComments::default())),
    };
    assert_eq!(columns.next(true), columns);
    let moved = columns.next(false);
    assert_eq!(moved.comments, None);
    assert!(moved.manual);
    assert_eq!(moved.rule, Some(3));
    assert_eq!(moved.before_comments, Some(ListMode::Brief));
}

#[test]
fn the_panel_sorts_by_its_comments_and_keeps_the_cursor_entry() {
    let path = VfsPath::local("/data");
    let mut panel = PanelState::showing(path, vec![file("a"), file("b"), file("c")]);
    panel.cursor = 2;
    panel.sort = SortOrder::default().choose(SortKey::Comment);
    panel.order_by_comments();
    let names = |panel: &PanelState| {
        panel
            .entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&panel), ["a", "b", "c"], "no comments read yet");

    let comments: FolderComments = [("a", "3"), ("b", "1"), ("c", "2")]
        .into_iter()
        .map(|(name, comment)| (name.to_owned(), comment.to_owned()))
        .collect();
    panel.columns.comments = Some(Arc::new(comments));
    panel.order_by_comments();
    assert_eq!(names(&panel), ["b", "c", "a"]);
    assert_eq!(panel.cursor, 1, "the cursor stays on c");
    assert_eq!(panel.comment("A"), Some("3"));
}
