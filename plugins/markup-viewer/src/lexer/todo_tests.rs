use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    Comment, Function, Invalid, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

/// A PlainTasks list with what sets it apart: projects (one with a tag), open tasks with every
/// mark, a done task by its mark with its date tags, one done by a tag only, a cancelled one,
/// tags with and without values, a note, and the archive below its rule.
const SAMPLE: &str = "Website: @work
  ☐ write the plan @high @due(26-05-12)
  - call Anna @started(26-05-10 09:00)
  * [ ] review
  [ ] order paper
  ✔ ship it @done(26-05-10 14:15) @lasted(1h)
  - old way @done(2026-05-01)
  ✘ dropped @cancelled(26-05-09 11:00)
  [x] booked @project(Website)
  a note with mail a@b.c and @tag
Done today:
--- ✄ -----------------------
Archive:";

#[test]
fn a_todo_list_is_coloured_like_plaintasks_colours_it() {
    let lines = lexed(Lang::Todo, SAMPLE);
    assert_eq!(
        lines[0],
        vec![
            ("Website", Keyword),
            (":", Punctuation),
            ("@work", Function)
        ]
    );
    assert_eq!(
        lines[1],
        vec![
            ("☐", Operator),
            ("@high", Function),
            ("@due", TypeName),
            ("(", Punctuation),
            ("26-05-12", Number),
            (")", Punctuation)
        ]
    );
    assert_eq!(
        lines[2],
        vec![
            ("-", Operator),
            ("@started", TypeName),
            ("(", Punctuation),
            ("26-05-10 09:00", Number),
            (")", Punctuation)
        ]
    );
    assert_eq!(lines[3], vec![("*", Operator)], "`[ ]` after `*` is text");
    assert_eq!(lines[4], vec![("[ ]", Operator)]);
    assert_eq!(
        lines[5],
        vec![
            ("✔", Literal),
            ("ship it", Comment),
            ("@done", TypeName),
            ("(", Punctuation),
            ("26-05-10 14:15", Number),
            (")", Punctuation),
            ("@lasted", TypeName),
            ("(", Punctuation),
            ("1h", Number),
            (")", Punctuation)
        ]
    );
    assert_eq!(
        lines[6][..3],
        [("-", Operator), ("old way", Comment), ("@done", TypeName)],
        "`@done` closes an open task"
    );
    assert_eq!(
        lines[7][..3],
        [
            ("✘", Invalid),
            ("dropped", Comment),
            ("@cancelled", TypeName)
        ]
    );
    assert_eq!(
        lines[8],
        vec![
            ("[x]", Literal),
            ("booked", Comment),
            ("@project", Function),
            ("(", Punctuation),
            ("Website", String),
            (")", Punctuation)
        ]
    );
    assert_eq!(
        lines[9],
        vec![("@tag", Function)],
        "a mail address is no tag"
    );
    assert_eq!(lines[10], vec![("Done today", Keyword), (":", Punctuation)]);
    assert_eq!(lines[11], vec![("--- ✄ -----------------------", Comment)]);
    assert_eq!(class_of(&lines[12..], "Archive"), Some(Keyword));
}

#[test]
fn a_mark_needs_a_blank_after_it_and_an_open_task_is_not_dimmed() {
    let lines = lexed(
        Lang::Todo,
        "-dash\n+ 1 vote\n- open task @today\n-\n☐\n  \n:\nnote ending in a colon: and more",
    );
    assert!(lines[0].is_empty(), "{:?}", lines[0]);
    assert_eq!(lines[1], vec![("+", Literal), ("1 vote", Comment)]);
    assert_eq!(texts_of(&lines[2..3], Comment), Vec::<&str>::new());
    assert_eq!(class_of(&lines[2..3], "@today"), Some(TypeName));
    assert_eq!(lines[3], vec![("-", Operator)]);
    assert_eq!(lines[4], vec![("☐", Operator)]);
    assert!(lines[5].is_empty());
    assert!(lines[6].is_empty(), "a colon alone is no project");
    assert!(lines[7].is_empty(), "a note");
}

#[test]
fn a_value_left_open_is_text_and_tags_need_a_name() {
    let lines = lexed(Lang::Todo, "✔ done @done(26-05-10 and @ alone @x()");
    assert_eq!(
        lines[0],
        vec![
            ("✔", Literal),
            ("done", Comment),
            ("@done", TypeName),
            ("(26-05-10 and @ alone", Comment),
            ("@x", Function),
            ("(", Punctuation),
            (")", Punctuation)
        ]
    );
}
