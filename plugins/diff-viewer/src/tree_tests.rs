use super::*;
use crate::Viewer;
use crate::test_support::{TestSource, source};

fn open(text: &str) -> Viewer<TestSource<Vec<u8>>> {
    Viewer::open(source(text)).expect("the document opens").0
}

/// (label, value, has children) of each node.
fn shown(nodes: &[TreeNode]) -> Vec<(String, Option<String>, bool)> {
    nodes
        .iter()
        .map(|node| (node.label.clone(), node.value.clone(), node.has_children))
        .collect()
}

fn node(label: &str, value: &str, children: bool) -> (String, Option<String>, bool) {
    (label.to_owned(), Some(value.to_owned()), children)
}

const GIT_PATCH: &str = concat!(
    "diff --git a/src/lib.rs b/src/lib.rs\n",
    "index 1111111..2222222 100644\n",
    "--- a/src/lib.rs\n",
    "+++ b/src/lib.rs\n",
    "@@ -1,2 +1,3 @@ mod a;\n",
    " a\n",
    "+b\n",
    " c\n",
    "@@ -10 +11 @@\n",
    "-x\n",
    "+y\n",
    "diff --git a/old name.txt b/new name.txt\n",
    "similarity index 90%\n",
    "rename from old name.txt\n",
    "rename to new name.txt\n",
    "--- a/old name.txt\n",
    "+++ b/new name.txt\n",
    "@@ -1 +1 @@\n",
    "-v1\n",
    "+v2\n",
    "diff --git a/logo.png b/logo.png\n",
    "index 3333333..4444444 100644\n",
    "Binary files a/logo.png and b/logo.png differ\n",
    "diff --git a/added.txt b/added.txt\n",
    "new file mode 100644\n",
    "index 0000000..5555555\n",
    "--- /dev/null\n",
    "+++ b/added.txt\n",
    "@@ -0,0 +1,2 @@\n",
    "+one\n",
    "+two\n",
    "diff --git a/gone.txt b/gone.txt\n",
    "deleted file mode 100644\n",
    "index 6666666..0000000\n",
    "--- a/gone.txt\n",
    "+++ /dev/null\n",
    "@@ -1 +0,0 @@\n",
    "-only\n",
);

#[test]
fn a_node_per_file_with_its_counts_and_what_happened_to_it() {
    let mut viewer = open(GIT_PATCH);
    let top = viewer.children(None).expect("the files");
    assert_eq!(
        shown(&top),
        vec![
            node("src/lib.rs", "+2 \u{2212}1", true),
            node(
                "new name.txt",
                "+1 \u{2212}1, renamed from old name.txt",
                true
            ),
            node("logo.png", "+0 \u{2212}0, binary", false),
            node("added.txt", "+2 \u{2212}0, new", true),
            node("gone.txt", "+0 \u{2212}1, deleted", true),
        ]
    );
    let hunks = viewer.children(Some(top[0].id)).expect("the hunks");
    assert_eq!(
        shown(&hunks),
        vec![
            node("@@ -1,2 +1,3 @@ mod a;", "3 lines", false),
            node("@@ -10 +11 @@", "2 lines", false),
        ]
    );
    assert!(viewer.children(Some(hunks[0].id)).expect("none").is_empty());
    assert!(viewer.children(Some(top[2].id)).expect("none").is_empty());
}

#[test]
fn a_copy_names_where_it_came_from() {
    let mut viewer = open(concat!(
        "diff --git a/a.txt b/c.txt\n",
        "similarity index 100%\n",
        "copy from a.txt\n",
        "copy to c.txt\n",
    ));
    let top = viewer.children(None).expect("the files");
    assert_eq!(
        shown(&top),
        vec![node("c.txt", "+0 \u{2212}0, copied from a.txt", false)]
    );
}

#[test]
fn files_of_a_plain_unified_diff_start_at_their_old_header() {
    let mut viewer = open(concat!(
        "--- a.txt\t2026-10-20 10:00:00\n",
        "+++ b.txt\t2026-10-20 10:01:00\n",
        "@@ -1 +1 @@\n",
        "-a\n",
        "+b\n",
        "--- c.txt\t2026-10-20 10:00:00\n",
        "+++ c.txt\t2026-10-20 10:01:00\n",
        "@@ -1,2 +1 @@\n",
        "-c\n",
        " d\n",
    ));
    let top = viewer.children(None).expect("the files");
    assert_eq!(
        shown(&top),
        vec![
            node("b.txt", "+1 \u{2212}1", true),
            node("c.txt", "+0 \u{2212}1", true),
        ]
    );
}

#[test]
fn a_context_diff_hunk_is_named_by_its_first_range() {
    let mut viewer = open(concat!(
        "*** a.txt\t2026-10-20 10:00:00\n",
        "--- b.txt\t2026-10-20 10:01:00\n",
        "***************\n",
        "*** 1,2 ****\n",
        "  same\n",
        "! old\n",
        "--- 1,2 ----\n",
        "  same\n",
        "! new\n",
    ));
    let top = viewer.children(None).expect("the files");
    assert_eq!(shown(&top), vec![node("b.txt", "+1 \u{2212}1", true)]);
    let hunks = viewer.children(Some(top[0].id)).expect("the hunks");
    assert_eq!(shown(&hunks), vec![node("*** 1,2 ****", "4 lines", false)]);
}

#[test]
fn many_files_are_shown_in_ranges() {
    let mut text = String::new();
    for index in 0..1500 {
        text.push_str(&format!(
            "--- f{index}\n+++ f{index}\n@@ -1 +1 @@\n-a\n+b\n"
        ));
    }
    let mut viewer = open(&text);
    let top = viewer.children(None).expect("the ranges");
    assert_eq!(
        top.iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        vec!["files 1..1000", "files 1001..1500"]
    );
    let second = viewer.children(Some(top[1].id)).expect("the files");
    assert_eq!(second.len(), 500);
    assert_eq!(second[0].label, "f1000");
    assert_eq!(second[499].label, "f1499");
}

#[test]
fn the_tree_stays_within_its_caps_and_says_what_it_left_out() {
    let mut text = String::new();
    for index in 0..MAX_FILES + 3 {
        text.push_str(&format!("--- f{index}\n+++ f{index}\n"));
    }
    let mut viewer = open(&text);
    assert_eq!(viewer.tree.file_count(), MAX_FILES);
    let top = viewer.children(None).expect("the ranges");
    assert_eq!(top.len(), MAX_FILES.div_ceil(PAGE) + 1);
    let last = top.last().expect("the note");
    assert_eq!(
        (last.label.as_str(), last.value.as_deref()),
        ("not listed", Some("3 more files, 0 more hunks"))
    );
}

#[test]
fn hunks_past_their_cap_are_counted_and_a_file_of_many_shows_ranges() {
    let text = "@@ -0,0 +0,0 @@\n".repeat(MAX_HUNKS + 2);
    let mut viewer = open(&text);
    let top = viewer.children(None).expect("the file");
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].label, "(unnamed)");
    assert_eq!(top[1].value.as_deref(), Some("0 more files, 2 more hunks"));
    let ranges = viewer.children(Some(top[0].id)).expect("the ranges");
    assert_eq!(ranges.len(), MAX_HUNKS.div_ceil(PAGE));
    assert_eq!(ranges[1].label, "hunks 1001..2000");
    let hunks = viewer.children(Some(ranges[1].id)).expect("the hunks");
    assert_eq!(hunks.len(), PAGE);
    assert_eq!(hunks[0].label, "@@ -0,0 +0,0 @@");
    assert_eq!(hunks[0].value.as_deref(), Some("0 lines"));
}

#[test]
fn an_unknown_node_is_not_found() {
    let mut viewer = open(GIT_PATCH);
    for id in [
        7,
        (FILE << KIND_SHIFT) | 99,
        (HUNK_RANGE << KIND_SHIFT) | (99 << FILE_SHIFT),
    ] {
        let failure = viewer.children(Some(id)).expect_err("no such node");
        assert_eq!(failure.kind, FailureKind::NotFound);
    }
}
