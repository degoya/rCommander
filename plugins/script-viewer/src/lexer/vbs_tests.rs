use super::date;
use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    Comment, Function, Keyword, Literal, Number, Operator, String, TypeName,
};

/// A VBScript file with what sets the language apart: keywords in any case, both comments, `""`
/// in a string, a date, `&H`, declared procedures and a class, `End Sub` declaring nothing,
/// `Property Get`, built-in functions, a continued line and a string its line leaves open.
const SAMPLE: &str = r#"Option Explicit
' a comment
Dim name, count : count = &H1F
Const START = #1/31/2026#
name = "say ""hi"""
Rem another comment
Function Greet(ByVal who)
    Greet = "Hello, " & who & _
        "!"
End Function
Class Shop
    Private Sub Class_Initialize()
        Set items = CreateObject("Scripting.Dictionary")
    End Sub
    Public Property Get Total
        Total = 1.5E3
    End Property
End Class
If IsEmpty(name) Then MsgBox Greet(name) Else WScript.Quit 1
x = "open"#;

#[test]
fn a_vbscript_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::VbScript, SAMPLE);
    assert_eq!(lines[0], vec![("Option", Keyword), ("Explicit", Keyword)]);
    assert_eq!(lines[1], vec![("' a comment", Comment)]);
    assert_eq!(class_of(&lines[2..3], "&H1F"), Some(Number));
    assert_eq!(class_of(&lines[2..3], ":"), Some(Operator));
    assert_eq!(class_of(&lines[3..4], "#1/31/2026#"), Some(Number));
    assert_eq!(texts_of(&lines[4..5], String), vec!["\"say \"\"hi\"\"\""]);
    assert_eq!(lines[5], vec![("Rem another comment", Comment)]);
    assert_eq!(
        texts_of(&lines[6..7], Function),
        vec!["Greet"],
        "the name after Function"
    );
    assert_eq!(class_of(&lines[6..7], "ByVal"), Some(Keyword));
    // The name used as the return value is a plain name again.
    assert_eq!(class_of(&lines[7..8], "Greet"), None);
    assert_eq!(class_of(&lines[7..8], "_"), Some(Operator));
    assert_eq!(
        lines[9],
        vec![("End", Keyword), ("Function", Keyword)],
        "End Function declares nothing"
    );
    assert_eq!(lines[10], vec![("Class", Keyword), ("Shop", TypeName)]);
    assert_eq!(class_of(&lines[11..12], "Class_Initialize"), Some(Function));
    assert_eq!(class_of(&lines[12..13], "Set"), Some(Keyword));
    assert_eq!(class_of(&lines[12..13], "CreateObject"), Some(Function));
    assert_eq!(
        lines[14],
        vec![
            ("Public", Keyword),
            ("Property", Keyword),
            ("Get", Keyword),
            ("Total", Function)
        ]
    );
    assert_eq!(texts_of(&lines[15..16], Number), vec!["1.5E3"]);
    assert_eq!(lines[17], vec![("End", Keyword), ("Class", Keyword)]);
    assert_eq!(
        texts_of(&lines[18..19], Function),
        vec!["IsEmpty", "MsgBox", "WScript"]
    );
    assert_eq!(texts_of(&lines[19..], String), vec!["\"open"]);
}

#[test]
fn keywords_and_constants_ignore_case() {
    let lines = lexed(
        Lang::VbScript,
        "IF x = NOTHING tHeN rem done\nREM all of it",
    );
    assert_eq!(texts_of(&lines[..1], Keyword), vec!["IF", "tHeN"]);
    assert_eq!(texts_of(&lines[..1], Literal), vec!["NOTHING"]);
    assert_eq!(texts_of(&lines, Comment), vec!["rem done", "REM all of it"]);
    // `Remark` is a name, not a comment.
    let lines = lexed(Lang::VbScript, "Remark = 1");
    assert!(texts_of(&lines, Comment).is_empty());
}

#[test]
fn a_date_needs_digits_and_its_closing_sign() {
    assert!(date(b"1/31/2026"));
    assert!(date(b"10:30 PM"));
    assert!(!date(b"abc"));
    assert!(!date(b""));
    let lines = lexed(Lang::VbScript, "x = #1/2/2003# + #no");
    assert_eq!(texts_of(&lines, Number), vec!["#1/2/2003#"]);
}
