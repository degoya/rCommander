use super::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

/// Both panels as Total Commander's help describes them: the source in `C:\Work` with `a b.txt`
/// under the cursor, the target in `D:\Out` with `x.log` under it.
fn context() -> ParamContext {
    ParamContext {
        source_dir: Some("C:\\Work".to_owned()),
        source_cursor: Some("a b.txt".to_owned()),
        source_marked: Vec::new(),
        target_dir: Some("D:\\Out\\".to_owned()),
        target_cursor: Some("x.log".to_owned()),
        target_marked: Vec::new(),
        lists: vec![(ListKind::PathsUtf8, "C:\\Temp\\list-utf8.txt".to_owned())],
        separator: '\\',
    }
}

fn windows(params: &str, context: &ParamContext) -> String {
    expand_windows(params, context)
        .expect("expands")
        .args
        .join("|")
}

#[test]
fn percent_variables_follow_total_commander() {
    let context = context();
    assert_eq!(windows("\"%P%N\"", &context), "\"C:\\Work\\a b.txt\"");
    assert_eq!(windows("%T%M", &context), "D:\\Out\\x.log");
    assert_eq!(windows("%O.%E", &context), "a b.txt");
    assert_eq!(windows("-o %O -e %E", &context), "-o a b -e txt");
    assert_eq!(
        windows("100%% %X", &context),
        "100% %X",
        "unknown names stay"
    );
    assert_eq!(
        windows("%n %p %t", &context),
        "%n %p %t",
        "no 8.3 variants: lower case stays as written"
    );
    assert_eq!(windows("%UL", &context), "C:\\Temp\\list-utf8.txt");
    assert_eq!(windows("", &context), "", "no parameters, no argument");
    assert_eq!(
        lists_in("%L %UF %L %WL %F %WF %UL"),
        [
            ListKind::PathsAnsi,
            ListKind::NamesUtf8,
            ListKind::PathsUtf16,
            ListKind::NamesAnsi,
            ListKind::NamesUtf16,
            ListKind::PathsUtf8,
        ]
    );
}

/// `%S`, `%P%S` and `%R` quote a name with a blank themselves; `%Q` switches that off.
#[test]
fn marked_names_are_quoted_unless_q() {
    let mut context = context();
    context.source_marked = names(&["one.txt", "two words.txt", "three x.txt"]);
    context.target_marked = names(&["t 1"]);
    assert_eq!(
        windows("%S", &context),
        "one.txt \"two words.txt\" \"three x.txt\""
    );
    assert_eq!(
        windows("%P%S", &context),
        "C:\\Work\\one.txt \"C:\\Work\\two words.txt\" \"C:\\Work\\three x.txt\""
    );
    assert_eq!(windows("%R", &context), "\"t 1\"");
    assert_eq!(windows("%T%R", &context), "\"D:\\Out\\t 1\"");
    assert_eq!(
        windows("%Q%S", &context),
        "one.txt two words.txt three x.txt"
    );
}

#[test]
fn nothing_marked_means_the_cursor() {
    let mut context = context();
    assert_eq!(windows("%S", &context), "\"a b.txt\"");
    assert_eq!(windows("%Y%S", &context), "", "%Y: an empty list");
    assert_eq!(context.names(PanelSide::Source, false), ["a b.txt"]);
    context.source_cursor = None;
    assert!(context.names(PanelSide::Source, false).is_empty());
    context.source_marked = names(&["m"]);
    assert_eq!(context.names(PanelSide::Source, true), ["m"]);
}

/// Elsewhere than Windows the parameters are split into words first: a name with blanks, quotes
/// or shell syntax stays one argument, and a word with `%S` becomes one per name.
#[test]
fn a_name_never_becomes_shell_syntax() {
    let context = ParamContext {
        source_dir: Some("/home/me".to_owned()),
        source_cursor: Some("x; rm -rf ~ $(id) 'q'.txt".to_owned()),
        source_marked: names(&["a b", "`c`", "d\"e"]),
        target_dir: Some("/tmp/out".to_owned()),
        target_cursor: None,
        target_marked: Vec::new(),
        lists: Vec::new(),
        separator: '/',
    };
    let expanded =
        expand_posix("--open=%P%N -v %P%S 'two words' end\\ here", &context).expect("expands");
    assert_eq!(
        expanded.args,
        [
            "--open=/home/me/x; rm -rf ~ $(id) 'q'.txt",
            "-v",
            "/home/me/a b",
            "/home/me/`c`",
            "/home/me/d\"e",
            "two words",
            "end here",
        ]
    );
    assert!(expanded.values.contains(&"/home/me/a b".to_owned()));
    let quoted = expand_posix("\"%T%M\" %S", &context).expect("expands");
    assert_eq!(quoted.args, ["/tmp/out/", "a b", "`c`", "d\"e"]);
    assert!(
        expand_posix("'open", &context).is_err(),
        "an unclosed quote"
    );
}

/// A placeholder of a panel that shows no local folder (an archive, a server) is refused.
#[test]
fn a_folder_that_is_not_local_is_refused() {
    let mut context = context();
    context.target_dir = None;
    let error = expand_windows("%T", &context).expect_err("not local");
    assert_eq!(error.code(), "user_command_local_only");
    assert_eq!(windows("%N", &context), "a b.txt", "a name still reads");
}

/// A name placeholder of a panel without a local folder answers `user_command_local_only` as
/// its folder does - never an empty name.
#[test]
fn names_of_a_panel_without_a_local_folder_are_refused() {
    let mut context = context();
    context.target_dir = None;
    for params in ["%M", "%R", "%T%R"] {
        let error = expand_windows(params, &context).expect_err("not local");
        assert_eq!(error.code(), "user_command_local_only", "{params}");
    }
    context.source_dir = None;
    for params in ["%N", "%O", "%E", "%S", "%P%S"] {
        let error = expand_windows(params, &context).expect_err("not local");
        assert_eq!(error.code(), "user_command_local_only", "{params}");
        let error = expand_posix(params, &context).expect_err("not local");
        assert_eq!(error.code(), "user_command_local_only", "{params}");
    }
}

/// A value with a double quote would end the quotes around it in a Windows command line and
/// add arguments of its own (`a" --evil "b`): the start is refused. Elsewhere each value is one
/// argument anyway.
#[test]
fn a_double_quote_never_reaches_a_windows_command_line() {
    let mut context = context();
    context.source_cursor = Some("a\" --evil \"b".to_owned());
    for params in ["\"%P%N\"", "%N", "%O", "%S", "%P%S"] {
        let error = expand_windows(params, &context).expect_err("a quote");
        assert_eq!(error.code(), "user_command_invalid", "{params}");
        assert!(error.to_string().contains("double quote"), "{params}");
    }
    let mut marked = context_with_marked(&["ok.txt", "x\"y"]);
    assert!(expand_windows("%S", &marked).is_err());
    marked.source_marked = names(&["ok.txt"]);
    assert!(expand_windows("%S", &marked).is_ok());
    let mut folder = self::context();
    folder.target_dir = Some("D:\\a\"b".to_owned());
    assert!(expand_windows("%T", &folder).is_err());
    let posix = expand_posix("%N", &context).expect("one argument");
    assert_eq!(posix.args, ["a\" --evil \"b"]);
}

/// [`context`] with `marked` marked in the source.
fn context_with_marked(marked: &[&str]) -> ParamContext {
    ParamContext {
        source_marked: names(marked),
        ..context()
    }
}
