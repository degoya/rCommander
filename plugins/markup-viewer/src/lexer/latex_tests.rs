use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Operator, Punctuation, String, TypeName,
};

/// A LaTeX file with what sets it apart: a class and a package, a comment, a definition, an
/// environment, a starred section and a label, inline math with commands, a citation with an
/// optional argument, display math over lines, a verbatim environment, `\verb`, control symbols,
/// a parameter, alignment, and math a blank line ends.
const SAMPLE: &str = r"\documentclass[11pt]{article}
\usepackage{amsmath} % maths
\newcommand{\R}{\mathbb{R}}
\begin{document}
\section*{Intro}\label{sec:intro}
Text with $x^2 + \alpha$ and \cite[p.~5]{knuth}.
\[
  \int_0^1 f(x)\,dx
\]
\begin{verbatim}
\notacommand $ % kept
\end{verbatim}
Use \verb|\foo| here, 50\% off \\
#1 and & ~
$open

\end{document}";

#[test]
fn a_latex_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::Latex, SAMPLE);
    assert_eq!(
        lines[0],
        vec![
            ("\\documentclass", Keyword),
            ("[", Punctuation),
            ("]", Punctuation),
            ("{", Punctuation),
            ("article", TypeName),
            ("}", Punctuation)
        ]
    );
    assert_eq!(class_of(&lines[1..2], "amsmath"), Some(TypeName));
    assert_eq!(class_of(&lines[1..2], "% maths"), Some(Comment));
    assert_eq!(class_of(&lines[2..3], "\\newcommand"), Some(Keyword));
    assert_eq!(class_of(&lines[2..3], "\\R"), Some(Function));
    assert_eq!(class_of(&lines[2..3], "\\mathbb"), Some(Keyword));
    assert_eq!(class_of(&lines[3..4], "document"), Some(TypeName));
    assert_eq!(
        lines[4],
        vec![
            ("\\section*", Keyword),
            ("{", Punctuation),
            ("}", Punctuation),
            ("\\label", Keyword),
            ("{", Punctuation),
            ("sec:intro", Key),
            ("}", Punctuation)
        ]
    );
    // Inline math: its text a string, its commands keywords, its delimiters operators.
    assert_eq!(texts_of(&lines[5..6], Operator), vec!["$", "^", "+", "$"]);
    assert_eq!(class_of(&lines[5..6], "\\alpha"), Some(Keyword));
    assert_eq!(class_of(&lines[5..6], "x"), Some(String));
    assert_eq!(class_of(&lines[5..6], "knuth"), Some(Key));
    assert_eq!(lines[6], vec![("\\[", Operator)]);
    assert_eq!(class_of(&lines[7..8], "\\int"), Some(Keyword));
    assert_eq!(class_of(&lines[7..8], "\\,"), Some(Operator));
    assert_eq!(class_of(&lines[7..8], "dx"), Some(String));
    assert_eq!(lines[8], vec![("\\]", Operator)]);
    assert_eq!(class_of(&lines[9..10], "verbatim"), Some(TypeName));
    // Inside the verbatim environment nothing is a command, math or a comment.
    assert_eq!(lines[10], vec![("\\notacommand $ % kept", String)]);
    assert_eq!(
        lines[11],
        vec![
            ("\\end", Keyword),
            ("{", Punctuation),
            ("verbatim", TypeName),
            ("}", Punctuation)
        ]
    );
    assert_eq!(
        lines[12],
        vec![
            ("\\verb", Keyword),
            ("|\\foo|", String),
            ("\\%", Operator),
            ("\\\\", Operator)
        ]
    );
    assert_eq!(
        lines[13],
        vec![("#1", Key), ("&", Operator), ("~", Operator)]
    );
    assert_eq!(lines[14], vec![("$", Operator), ("open", String)]);
    // The blank line ended the math: the last line is text again.
    assert_eq!(class_of(&lines[16..], "document"), Some(TypeName));
}

#[test]
fn package_commands_hold_at_signs_and_comments_follow_no_escaped_percent() {
    let lines = lexed(
        Lang::Latex,
        "\\def\\my@cmd#1{\\@ifnextchar[}% note\n\\input{chapters/one} \\$5",
    );
    assert_eq!(class_of(&lines, "\\def"), Some(Keyword));
    assert_eq!(class_of(&lines, "\\my@cmd"), Some(Function));
    assert_eq!(class_of(&lines, "\\@ifnextchar"), Some(Keyword));
    assert_eq!(class_of(&lines, "% note"), Some(Comment));
    assert_eq!(class_of(&lines, "chapters/one"), Some(String));
    assert_eq!(class_of(&lines, "\\$"), Some(Operator));
    assert!(texts_of(&lines[1..], Comment).is_empty());
}

#[test]
fn display_math_in_dollars_goes_on_over_lines() {
    let lines = lexed(Lang::Latex, "$$ a\n= b $$ text");
    assert_eq!(lines[0], vec![("$$", Operator), (" a", String)]);
    assert_eq!(
        lines[1],
        vec![("=", Operator), (" b ", String), ("$$", Operator)]
    );
}
