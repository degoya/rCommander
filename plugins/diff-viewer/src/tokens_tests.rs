use super::*;
use crate::test_support::highlighted;
use crate::types::TokenClass::{Comment, Function, Invalid, Keyword, Number, TypeName};

fn t(class: TokenClass, text: &str) -> (TokenClass, String) {
    (class, text.to_owned())
}

fn lines(text: &str) -> Vec<Vec<(TokenClass, String)>> {
    highlighted(text.as_bytes())
}

#[test]
fn a_git_diff_colours_every_kind_of_line() {
    let got = lines(concat!(
        "diff --git a/src/main.rs b/src/main.rs\n",
        "index 83db48f..bf269f4 100644\n",
        "--- a/src/main.rs\n",
        "+++ b/src/main.rs\n",
        "@@ -1,3 +1,4 @@ fn main() {\n",
        " unchanged\n",
        "-removed\n",
        "+added\n",
        "+second \u{e9}\n",
        " tail\n",
        "\\ No newline at end of file\n",
    ));
    assert_eq!(
        got,
        vec![
            vec![t(TypeName, "diff --git a/src/main.rs b/src/main.rs")],
            vec![t(TypeName, "index 83db48f..bf269f4 100644")],
            vec![t(TypeName, "--- a/src/main.rs")],
            vec![t(TypeName, "+++ b/src/main.rs")],
            vec![t(Keyword, "@@ -1,3 +1,4 @@"), t(Function, "fn main() {")],
            vec![],
            vec![t(Invalid, "-removed")],
            vec![t(Number, "+added")],
            vec![t(Number, "+second \u{e9}")],
            vec![],
            vec![t(Comment, "\\ No newline at end of file")],
        ]
    );
}

#[test]
fn git_extended_headers_are_type_names() {
    let text = concat!(
        "diff --git a/old.txt b/new.txt\n",
        "similarity index 90%\n",
        "dissimilarity index 10%\n",
        "rename from old.txt\n",
        "rename to new.txt\n",
        "diff --git a/a.txt b/c.txt\n",
        "copy from a.txt\n",
        "copy to c.txt\n",
        "diff --git a/gone.txt b/gone.txt\n",
        "deleted file mode 100644\n",
        "diff --git a/img.png b/img.png\n",
        "new file mode 100644\n",
        "Binary files /dev/null and b/img.png differ\n",
        "diff --git a/run.sh b/run.sh\n",
        "old mode 100644\n",
        "new mode 100755\n",
        "GIT binary patch\n",
    );
    for (line, got) in text.lines().zip(lines(text)) {
        assert_eq!(got, vec![t(TypeName, line)], "{line}");
    }
}

#[test]
fn a_format_patch_mail_header_is_a_comment() {
    let got = lines(concat!(
        "From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n",
        "From: Jane Doe <jane@example.org>\n",
        "Subject: [PATCH] Fix the thing\n",
        "\n",
        "index of the message, not a header\n",
        "---\n",
        " src/a.rs | 2 +-\n",
        "diff --git a/src/a.rs b/src/a.rs\n",
        "--- a/src/a.rs\n",
        "+++ b/src/a.rs\n",
        "@@ -1 +1 @@\n",
        "-old\n",
        "+new\n",
        "-- \n",
        "2.43.0\n",
    ));
    let comment = |text: &str| vec![t(Comment, text)];
    assert_eq!(
        got,
        vec![
            comment("From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001"),
            comment("From: Jane Doe <jane@example.org>"),
            comment("Subject: [PATCH] Fix the thing"),
            vec![],
            comment("index of the message, not a header"),
            comment("---"),
            comment(" src/a.rs | 2 +-"),
            vec![t(TypeName, "diff --git a/src/a.rs b/src/a.rs")],
            vec![t(TypeName, "--- a/src/a.rs")],
            vec![t(TypeName, "+++ b/src/a.rs")],
            vec![t(Keyword, "@@ -1 +1 @@")],
            vec![t(Invalid, "-old")],
            vec![t(Number, "+new")],
            // The signature after the last hunk.
            comment("-- "),
            comment("2.43.0"),
        ]
    );
}

#[test]
fn a_combined_diff_reads_a_column_per_parent() {
    let got = lines(concat!(
        "diff --cc file.txt\n",
        "index 1111111,2222222..3333333\n",
        "--- a/file.txt\n",
        "+++ b/file.txt\n",
        "@@@ -1,4 -1,5 +1,4 @@@ intro\n",
        "  both\n",
        "--gone from both\n",
        "- from ours\n",
        " -from theirs\n",
        "++merged\n",
        "+ ours kept\n",
        "  end\n",
        "--- a/next.txt\n",
    ));
    assert_eq!(
        got,
        vec![
            vec![t(TypeName, "diff --cc file.txt")],
            vec![t(TypeName, "index 1111111,2222222..3333333")],
            vec![t(TypeName, "--- a/file.txt")],
            vec![t(TypeName, "+++ b/file.txt")],
            vec![t(Keyword, "@@@ -1,4 -1,5 +1,4 @@@"), t(Function, "intro")],
            vec![],
            vec![t(Invalid, "--gone from both")],
            vec![t(Invalid, "- from ours")],
            vec![t(Invalid, " -from theirs")],
            vec![t(Number, "++merged")],
            vec![t(Number, "+ ours kept")],
            vec![],
            // The counts are used up: the next file.
            vec![t(TypeName, "--- a/next.txt")],
        ]
    );
}

#[test]
fn a_context_diff_takes_the_same_classes() {
    let got = lines(concat!(
        "*** a.txt\t2026-10-20 10:00:00\n",
        "--- b.txt\t2026-10-20 10:01:00\n",
        "***************\n",
        "*** 1,3 ****\n",
        "  same\n",
        "! old text\n",
        "- gone\n",
        "--- 1,3 ----\n",
        "  same\n",
        "! new text\n",
        "+ came\n",
    ));
    assert_eq!(
        got,
        vec![
            vec![t(TypeName, "*** a.txt\t2026-10-20 10:00:00")],
            vec![t(TypeName, "--- b.txt\t2026-10-20 10:01:00")],
            vec![t(Keyword, "***************")],
            vec![t(Keyword, "*** 1,3 ****")],
            vec![],
            vec![t(Invalid, "! old text")],
            vec![t(Invalid, "- gone")],
            vec![t(Keyword, "--- 1,3 ----")],
            vec![],
            vec![t(Number, "! new text")],
            vec![t(Number, "+ came")],
        ]
    );
}

#[test]
fn a_line_starting_with_two_dashes_inside_a_hunk_is_counted_not_guessed() {
    // `--- x` and `+++ x` are a removed and an added line while the `@@` header still promises
    // lines; the same text after the hunk is the next file's header.
    let got = lines(concat!(
        "--- a/q.sql\n",
        "+++ b/q.sql\n",
        "@@ -1,3 +1,2 @@\n",
        "--- the old comment\n",
        "+++ the new comment\n",
        " select 1;\n",
        "-- trailing\n",
        "--- a/next.sql\n",
        "+++ b/next.sql\n",
    ));
    assert_eq!(got[3], vec![t(Invalid, "--- the old comment")]);
    assert_eq!(got[4], vec![t(Number, "+++ the new comment")]);
    assert_eq!(got[5], vec![]);
    assert_eq!(got[6], vec![t(Invalid, "-- trailing")]);
    assert_eq!(got[7], vec![t(TypeName, "--- a/next.sql")]);
    assert_eq!(got[8], vec![t(TypeName, "+++ b/next.sql")]);
}

#[test]
fn no_newline_markers_inside_and_after_a_hunk_are_comments() {
    let got = lines(concat!(
        "@@ -1 +1 @@\n",
        "-a\n",
        "\\ No newline at end of file\n",
        "+b\n",
        "\\ No newline at end of file\n",
    ));
    assert_eq!(
        got[1..],
        [
            vec![t(Invalid, "-a")],
            vec![t(Comment, "\\ No newline at end of file")],
            vec![t(Number, "+b")],
            vec![t(Comment, "\\ No newline at end of file")],
        ]
    );
}

#[test]
fn crlf_line_ends_stay_out_of_the_tokens() {
    let got = lines("--- a\r\n+++ b\r\n@@ -1 +1 @@ ctx\r\n-x\r\n+y\r\n");
    assert_eq!(
        got,
        vec![
            vec![t(TypeName, "--- a")],
            vec![t(TypeName, "+++ b")],
            vec![t(Keyword, "@@ -1 +1 @@"), t(Function, "ctx")],
            vec![t(Invalid, "-x")],
            vec![t(Number, "+y")],
        ]
    );
}

#[test]
fn a_long_line_is_coloured_up_to_the_limit_and_the_hunk_goes_on() {
    // 200 KiB of two-byte characters: the token ends on the last whole one within 64 KiB.
    let long = format!("+{}", "\u{e9}".repeat(100_000));
    let text = format!("@@ -0,0 +1,2 @@\n{long}\n+short\n");
    let (mut viewer, _) =
        crate::Viewer::open(crate::test_support::source(&text)).expect("the document opens");
    let tokens = viewer.tokens(0, 3).expect("tokens");
    assert_eq!(
        tokens[1],
        vec![Token {
            start: 0,
            len: 65535,
            class: Number,
        }]
    );
    assert_eq!(
        tokens[2],
        vec![Token {
            start: 0,
            len: 6,
            class: Number,
        }]
    );
    assert!(tokens.iter().all(|line| line.len() <= MAX_TOKENS_PER_LINE));
}

#[test]
fn a_header_without_its_closing_at_signs_is_text() {
    let got = lines("diff -u a b\n@@ -1,2 +1,2\n@@ garbage @@ x\n+in a loose hunk\n");
    assert_eq!(got[1], vec![t(Comment, "@@ -1,2 +1,2")]);
    assert_eq!(got[2], vec![t(Keyword, "@@ garbage @@"), t(Function, "x")]);
    assert_eq!(got[3], vec![t(Number, "+in a loose hunk")]);
}
