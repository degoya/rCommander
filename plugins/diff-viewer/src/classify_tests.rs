use super::*;

/// The kinds of `lines` from the start of a file, and the state after the last.
fn run(lines: &[&str]) -> (Vec<Kind>, LineState) {
    let mut state = LineState::default();
    let kinds = lines
        .iter()
        .map(|line| classify(&mut state, line.as_bytes()))
        .collect();
    (kinds, state)
}

#[test]
fn a_hunk_header_promises_its_lines() {
    let (kinds, state) = run(&["@@ -10,2 +10,3 @@ fn f()"]);
    assert_eq!(
        kinds,
        vec![Kind::HunkHead {
            end: 17,
            context: 18
        }]
    );
    assert_eq!(
        state,
        LineState::Hunk {
            parents: 1,
            old: 2,
            new: 3
        }
    );
    // A range without a count is one line.
    assert_eq!(
        run(&["@@ -1 +1 @@"]).1,
        LineState::Hunk {
            parents: 1,
            old: 1,
            new: 1
        }
    );
    // Nothing promised: the hunk is over before it starts.
    assert_eq!(run(&["@@ -0,0 +0,0 @@"]).1, LineState::Files);
}

#[test]
fn counts_that_do_not_parse_make_a_loose_hunk() {
    for header in [
        "@@ -1,x +1 @@",
        "@@ -1 @@",
        "@@ -1 +1 +2 @@",
        "@@ -99999999999 +1 @@",
    ] {
        assert_eq!(
            run(&[header]).1,
            LineState::Loose { parents: 1 },
            "{header}"
        );
    }
    // No closing `@@`, or more parents than kept: no header at all.
    let many = format!("{} -1 +1 {}", "@".repeat(70), "@".repeat(70));
    for line in ["@@ -1 +1", "@ -1 +1 @", many.as_str()] {
        assert_eq!(run(&["diff x", line]).0[1], Kind::Text, "{line}");
    }
}

#[test]
fn a_hunk_ends_when_its_counts_are_used_up() {
    let (kinds, state) = run(&["@@ -1,2 +1,2 @@", " a", "-b", "+c", "--- next"]);
    assert_eq!(
        kinds[1..],
        [
            Kind::Unchanged,
            Kind::Removed,
            Kind::Added,
            Kind::Header(Header::Minus)
        ]
    );
    assert_eq!(state, LineState::Files);
}

#[test]
fn a_line_the_hunk_did_not_promise_ends_it_early() {
    let (kinds, state) = run(&["@@ -1,5 +1,5 @@", " a", "diff --git a/x b/x"]);
    assert_eq!(kinds[2], Kind::DiffStart);
    assert_eq!(state, LineState::Files);
    // A removed line when the old side is used up is no line of this hunk either.
    let (kinds, _) = run(&["@@ -1 +1,2 @@", "-a", "-b"]);
    assert_eq!(kinds[2], Kind::Text);
}

#[test]
fn an_empty_line_inside_a_hunk_is_an_unchanged_one() {
    let (kinds, state) = run(&["@@ -1,2 +1,2 @@", "", " b"]);
    assert_eq!(kinds[1..], [Kind::Unchanged, Kind::Unchanged]);
    assert_eq!(state, LineState::Files);
}

#[test]
fn a_combined_hunk_counts_every_parent() {
    let (_, state) = run(&["@@@ -1,2 -1,1 +1,2 @@@", "  both"]);
    assert_eq!(
        state,
        LineState::Hunk {
            parents: 2,
            old: 1,
            new: 1
        }
    );
    // `- ` is in the first parent alone, `++` in the result alone; then the hunk is done.
    let (kinds, state) = run(&["@@@ -1,2 -1,1 +1,2 @@@", "  both", "- ours", "++ new"]);
    assert_eq!(kinds[2..], [Kind::Removed, Kind::Added]);
    assert_eq!(state, LineState::Files);
    // A column that is none of ` +-` is no line of the hunk.
    assert_eq!(run(&["@@@ -1 -1 +1 @@@", "x+a"]).0[1], Kind::Text);
}

#[test]
fn a_loose_hunk_lasts_while_lines_look_like_its_body() {
    let (kinds, state) = run(&["@@ x @@", "+a", "-b", " c", "@@ -1 +1 @@"]);
    assert_eq!(kinds[1..4], [Kind::Added, Kind::Removed, Kind::Unchanged]);
    assert!(matches!(kinds[4], Kind::HunkHead { .. }));
    assert!(matches!(state, LineState::Hunk { .. }));
}

#[test]
fn the_preamble_takes_no_git_header() {
    let (kinds, state) = run(&["index 1..2", "+++ b", "new file mode 100644"]);
    assert_eq!(kinds, vec![Kind::Text; 3]);
    assert_eq!(state, LineState::Preamble);
    let (kinds, _) = run(&["--- a", "+++ b", "new file mode 100644"]);
    assert_eq!(
        kinds,
        vec![
            Kind::Header(Header::Minus),
            Kind::Header(Header::Plus),
            Kind::Header(Header::NewFile)
        ]
    );
}

#[test]
fn a_context_diff_follows_its_markers() {
    let (kinds, state) = run(&[
        "*** a",
        "--- b",
        "***************",
        "*** 1 ****",
        "! x",
        "--- 1 ----",
        "! y",
        "",
        "*** c",
    ]);
    assert_eq!(
        kinds,
        vec![
            Kind::Header(Header::Star),
            Kind::Header(Header::Minus),
            Kind::ContextHunk,
            Kind::ContextRange,
            Kind::Removed,
            Kind::ContextRange,
            Kind::Added,
            Kind::Unchanged,
            Kind::Header(Header::Star),
        ]
    );
    assert_eq!(state, LineState::Files);
}
