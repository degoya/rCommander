use super::{adornment, list_marker, table_border};
use crate::lang::Lang;
use crate::test_support::lexed;
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

/// A reStructuredText file with what sets the format apart: an overlined title and an underlined
/// one, a field, inline markup of every kind, a bullet and an enumerator, a literal block after
/// `::`, a directive with an option and content, a code block, a target, a footnote, a comment
/// over two lines, a doctest and a table border.
const SAMPLE: &str = "==========
User guide
==========

:Author: Ada

Intro with **strong**, *emphasis*, ``code`` and `a link <https://x.org>`_.
See :ref:`install` and target_ or |sub| [1]_.

Install
-------

- first item
2. second item

Example::

    $ make
    done

.. note:: Mind the gap.
   :class: tip

   Body *text*.

.. code-block:: python

   print(\"hi\")

.. _install: https://example.org
.. [1] A footnote.
.. a comment
   continued

>>> 1 + 1

+-----+-----+
Plain text.";

#[test]
fn a_rst_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::Rst, SAMPLE);
    assert_eq!(lines[0], vec![("==========", Punctuation)]);
    assert_eq!(lines[1], vec![("User guide", Keyword)]);
    assert_eq!(lines[2], vec![("==========", Punctuation)]);
    assert_eq!(lines[4], vec![(":Author:", Key)]);
    assert_eq!(
        lines[6],
        vec![
            ("**strong**", Keyword),
            ("*emphasis*", Literal),
            ("``code``", String),
            ("`a link <https://x.org>`_", TypeName)
        ]
    );
    assert_eq!(
        lines[7],
        vec![
            (":ref:", Function),
            ("`install`", String),
            ("target_", TypeName),
            ("|sub|", Key),
            ("[1]_", Key)
        ]
    );
    assert_eq!(lines[9], vec![("Install", Keyword)]);
    assert_eq!(lines[10], vec![("-------", Punctuation)]);
    assert_eq!(lines[12], vec![("-", Operator)]);
    assert_eq!(lines[13], vec![("2.", Number)]);
    assert_eq!(lines[15], vec![("::", Punctuation)]);
    assert_eq!(lines[17], vec![("$ make", String)]);
    assert_eq!(lines[18], vec![("done", String)]);
    assert_eq!(lines[20], vec![("..", Punctuation), ("note::", Keyword)]);
    assert_eq!(lines[21], vec![(":class:", Key)]);
    assert_eq!(lines[23], vec![("*text*", Literal)]);
    assert_eq!(
        lines[25],
        vec![
            ("..", Punctuation),
            ("code-block::", Keyword),
            ("python", TypeName)
        ]
    );
    assert_eq!(lines[27], vec![("print(\"hi\")", String)]);
    assert_eq!(
        lines[29],
        vec![
            ("..", Punctuation),
            ("_install:", Key),
            ("https://example.org", TypeName)
        ]
    );
    assert_eq!(lines[30], vec![("..", Punctuation), ("[1]", Key)]);
    assert_eq!(lines[31], vec![(".. a comment", Comment)]);
    assert_eq!(lines[32], vec![("continued", Comment)]);
    assert_eq!(lines[34], vec![(">>>", Operator), ("1 + 1", String)]);
    assert_eq!(lines[36], vec![("+-----+-----+", Punctuation)]);
    assert!(lines[37].is_empty());
}

#[test]
fn a_short_title_needs_an_underline_as_long_and_a_dedent_ends_a_block() {
    let lines = lexed(Lang::Rst, "Go\n==\n\nA\n-\ntext\n  ---\n");
    assert_eq!(lines[0], vec![("Go", Keyword)]);
    assert_eq!(lines[1], vec![("==", Punctuation)]);
    assert_eq!(lines[3], vec![("A", Keyword)]);
    assert_eq!(lines[4], vec![("-", Punctuation)]);
    // An indented line of dashes underlines nothing.
    assert!(lines[5].is_empty());
    let lines = lexed(Lang::Rst, "::\n\n  literal\nback *here*\n");
    assert_eq!(lines[2], vec![("literal", String)]);
    assert_eq!(lines[3], vec![("*here*", Literal)]);
}

#[test]
fn adornments_markers_and_borders() {
    assert!(adornment(b"====", 3));
    assert!(adornment(b"~~", 2));
    assert!(!adornment(b"~~", 3));
    assert!(!adornment(b"=-=", 1));
    assert!(!adornment(b"abc", 1));
    assert!(!adornment(b"", 0));
    assert_eq!(list_marker(b"- item"), Some((1, false)));
    assert_eq!(list_marker(b"* item"), Some((1, false)));
    assert_eq!(list_marker(b"12. item"), Some((3, true)));
    assert_eq!(list_marker(b"#. item"), Some((2, true)));
    assert_eq!(list_marker(b"(iv) item"), Some((4, true)));
    assert_eq!(list_marker(b"a) item"), Some((2, true)));
    assert_eq!(list_marker(b"| line"), Some((1, false)));
    assert_eq!(list_marker(b"*emphasis*"), None);
    assert_eq!(list_marker(b"Mr. Smith"), None);
    assert_eq!(list_marker(b"-1 is negative"), None);
    assert!(table_border(b"+----+----+"));
    assert!(table_border(b"=====  ====="));
    assert!(!table_border(b"a = b"));
    assert!(!table_border(b"+-"));
}
