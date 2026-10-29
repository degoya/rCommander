use super::*;
use crate::types::TokenClass::{Comment, Function, Key, Keyword, Number, Operator, Punctuation};

const STRING: TokenClass = TokenClass::String;

/// The tokens of each line of `text`, as the text they cover, lexed in order from the start.
fn lines(text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let mut tokens = Vec::new();
            state = lex_line(line.as_bytes(), state, Some(&mut tokens));
            tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    let end = start + token.len as usize;
                    (
                        line.get(start..end).expect("on character boundaries"),
                        token.class,
                    )
                })
                .collect()
        })
        .collect()
}

fn one(line: &str) -> Vec<(&str, TokenClass)> {
    lines(line).remove(0)
}

#[test]
fn commands_are_keywords_in_any_case_and_echo_text_stays_plain() {
    assert_eq!(
        one("@ECHO OFF"),
        vec![("@", Punctuation), ("ECHO", Keyword)]
    );
    for command in [
        "if", "else", "for", "in", "do", "goto", "call", "set", "setlocal", "endlocal", "echo",
        "exit", "shift", "pushd", "popd", "cd", "chdir", "start", "title", "color", "cls", "pause",
        "choice",
    ] {
        assert_eq!(one(command), vec![(command, Keyword)], "{command}");
        let upper = command.to_ascii_uppercase();
        assert_eq!(one(&upper), vec![(upper.as_str(), Keyword)], "{upper}");
    }
    assert_eq!(
        one("echo if not exist 1 \"x\" goto"),
        vec![("echo", Keyword)]
    );
    assert_eq!(one("echo."), vec![("echo", Keyword)]);
    assert_eq!(one("cd.."), vec![("cd", Keyword)]);
    assert_eq!(
        one("set/a x=1"),
        vec![("set", Keyword), ("/a", Keyword), ("1", Number)]
    );
    assert_eq!(one("timeout 5"), vec![("5", Number)]);
    assert_eq!(
        one("exit /b 0x1F"),
        vec![("exit", Keyword), ("0x1F", Number)]
    );
    assert!(one("echoes").is_empty());
}

#[test]
fn switches_are_keywords() {
    assert_eq!(
        one("set /p name=Name: "),
        vec![("set", Keyword), ("/p", Keyword)]
    );
    assert_eq!(
        one("for /D /R /L /F"),
        vec![
            ("for", Keyword),
            ("/D", Keyword),
            ("/R", Keyword),
            ("/L", Keyword),
            ("/F", Keyword),
        ]
    );
    assert!(one("dir /s").is_empty());
}

#[test]
fn rem_and_double_colon_are_comments_but_remark_is_not() {
    assert_eq!(one("rem a & b | c"), vec![("rem a & b | c", Comment)]);
    assert_eq!(one("REM"), vec![("REM", Comment)]);
    assert_eq!(one("rem.text"), vec![("rem.text", Comment)]);
    assert_eq!(one("rem."), vec![("rem.", Comment)]);
    assert_eq!(one("  :: note %x%"), vec![(":: note %x%", Comment)]);
    assert_eq!(
        one("echo a & rem b"),
        vec![("echo", Keyword), ("&", Operator), ("rem b", Comment)]
    );
    assert_eq!(one("remark 1"), vec![("1", Number)]);
    // A comment ends with its line, `^` or not.
    assert_eq!(
        lines("rem a ^\nif"),
        vec![vec![("rem a ^", Comment)], vec![("if", Keyword)]]
    );
}

#[test]
fn labels_are_functions_also_after_goto_and_call() {
    assert_eq!(one(":start"), vec![(":start", Function)]);
    assert_eq!(one(":end rest is ignored"), vec![(":end", Function)]);
    assert_eq!(
        one("goto start"),
        vec![("goto", Keyword), ("start", Function)]
    );
    assert_eq!(
        one("GOTO :EOF"),
        vec![("GOTO", Keyword), (":EOF", Function)]
    );
    assert_eq!(
        one("call :sub \"a\" 2"),
        vec![
            ("call", Keyword),
            (":sub", Function),
            ("\"a\"", STRING),
            ("2", Number),
        ]
    );
    // `call` with a command calls the command.
    assert_eq!(one("call other.bat"), vec![("call", Keyword)]);
    assert_eq!(
        one("call set x=1"),
        vec![("call", Keyword), ("set", Keyword), ("1", Number)]
    );
    assert_eq!(
        one("goto %target%"),
        vec![("goto", Keyword), ("%target%", Key)]
    );
}

#[test]
fn variables_are_keys() {
    assert_eq!(
        one("echo %PATH% %~dp0 %1 %* !x! %x:a=b% %x:~0,3% %~$PATH:1 %~nx2"),
        vec![
            ("echo", Keyword),
            ("%PATH%", Key),
            ("%~dp0", Key),
            ("%1", Key),
            ("%*", Key),
            ("!x!", Key),
            ("%x:a=b%", Key),
            ("%x:~0,3%", Key),
            ("%~$PATH:1", Key),
            ("%~nx2", Key),
        ]
    );
    // `%%` is a percent sign, `!` without its pair or with a blank inside is text.
    assert_eq!(one("echo 100%% sure, 5%% more"), vec![("echo", Keyword)]);
    assert_eq!(one("echo Done! Really!"), vec![("echo", Keyword)]);
}

#[test]
fn for_variables_and_its_keywords() {
    assert_eq!(
        one("for %%i in (*.txt) do echo %%i"),
        vec![
            ("for", Keyword),
            ("%%i", Key),
            ("in", Keyword),
            ("(", Punctuation),
            (")", Punctuation),
            ("do", Keyword),
            ("echo", Keyword),
            ("%%i", Key),
        ]
    );
    assert_eq!(
        one("FOR /F \"tokens=1,2 delims=,\" %%a IN ('type x') DO set %%~na=1"),
        vec![
            ("FOR", Keyword),
            ("/F", Keyword),
            ("\"tokens=1,2 delims=,\"", STRING),
            ("%%a", Key),
            ("IN", Keyword),
            ("(", Punctuation),
            (")", Punctuation),
            ("DO", Keyword),
            ("set", Keyword),
            ("%%~na", Key),
            ("1", Number),
        ]
    );
}

#[test]
fn delayed_expansion() {
    assert_eq!(
        lines("setlocal enabledelayedexpansion\nset /a n=!n!+1\necho !n:~0,2!"),
        vec![
            vec![("setlocal", Keyword)],
            vec![("set", Keyword), ("/a", Keyword), ("!n!", Key)],
            vec![("echo", Keyword), ("!n:~0,2!", Key)],
        ]
    );
}

#[test]
fn strings_are_split_around_their_variables() {
    assert_eq!(
        one("start \"\" \"C:\\Program Files\\%app%.exe\""),
        vec![
            ("start", Keyword),
            ("\"\"", STRING),
            ("\"C:\\Program Files\\", STRING),
            ("%app%", Key),
            (".exe\"", STRING),
        ]
    );
    assert_eq!(
        one("set \"x=a | b"),
        vec![("set", Keyword), ("\"x=a | b", STRING)]
    );
}

#[test]
fn if_conditions_and_comparisons() {
    assert_eq!(
        one("if /i \"%a%\"==\"b\" goto end"),
        vec![
            ("if", Keyword),
            ("/i", Keyword),
            ("\"", STRING),
            ("%a%", Key),
            ("\"", STRING),
            ("==", Operator),
            ("\"b\"", STRING),
            ("goto", Keyword),
            ("end", Function),
        ]
    );
    for comparison in ["equ", "NEQ", "lss", "leq", "gtr", "geq"] {
        assert_eq!(
            one(&format!("if %a% {comparison} 1 echo x")),
            vec![
                ("if", Keyword),
                ("%a%", Key),
                (comparison, Operator),
                ("1", Number),
                ("echo", Keyword),
            ],
            "{comparison}"
        );
    }
    assert_eq!(
        one("if not exist \"%~dp0x.txt\" exit /b 1"),
        vec![
            ("if", Keyword),
            ("not", Keyword),
            ("exist", Keyword),
            ("\"", STRING),
            ("%~dp0", Key),
            ("x.txt\"", STRING),
            ("exit", Keyword),
            ("1", Number),
        ]
    );
    assert_eq!(
        one("if errorlevel 1 if defined x if cmdextversion 2 cls"),
        vec![
            ("if", Keyword),
            ("errorlevel", Keyword),
            ("1", Number),
            ("if", Keyword),
            ("defined", Keyword),
            ("if", Keyword),
            ("cmdextversion", Keyword),
            ("2", Number),
            ("cls", Keyword),
        ]
    );
    // Outside `if` the words are plain.
    assert_eq!(one("set x=not"), vec![("set", Keyword)]);
}

#[test]
fn operators_and_redirections() {
    assert_eq!(
        one("dir | find \"x\" && echo ok || echo no & cls"),
        vec![
            ("|", Operator),
            ("\"x\"", STRING),
            ("&&", Operator),
            ("echo", Keyword),
            ("||", Operator),
            ("echo", Keyword),
            ("&", Operator),
            ("cls", Keyword),
        ]
    );
    assert_eq!(
        one("dir > out.txt 2>&1"),
        vec![(">", Operator), ("2>&1", Operator)]
    );
    assert_eq!(one("del x 2>nul"), vec![("2>nul", Operator)]);
    assert_eq!(one("del x > NUL"), vec![(">", Operator), ("NUL", Operator)]);
    assert_eq!(
        one("echo hi >> \"%log%\""),
        vec![
            ("echo", Keyword),
            (">>", Operator),
            ("\"", STRING),
            ("%log%", Key),
            ("\"", STRING),
        ]
    );
    assert_eq!(one("type a <in.txt"), vec![("<", Operator)]);
    assert_eq!(
        one(">nul echo x"),
        vec![(">nul", Operator), ("echo", Keyword)]
    );
    // `2>` redirects only as a word of its own; a handle goes to a handle with `>&`.
    assert_eq!(
        one("echo a2>b 1>&2"),
        vec![("echo", Keyword), (">", Operator), ("1>&2", Operator)]
    );
}

#[test]
fn echo_takes_operators_in_quotes_and_escaped_as_text() {
    assert_eq!(
        one("echo \"a | b & c > d\" | more"),
        vec![("echo", Keyword), ("|", Operator)]
    );
    assert_eq!(
        one("echo \"%x%\" ^& ^| ^> done"),
        vec![("echo", Keyword), ("%x%", Key)]
    );
    assert_eq!(one("echo(a) b"), vec![("echo", Keyword)]);
}

#[test]
fn blocks_reach_over_lines() {
    let text = "if exist x (\n    echo in (a^)\n) else (\n    echo b)\n  )\necho c)";
    assert_eq!(
        lines(text),
        vec![
            vec![("if", Keyword), ("exist", Keyword), ("(", Punctuation)],
            vec![("echo", Keyword)],
            vec![(")", Punctuation), ("else", Keyword), ("(", Punctuation)],
            // Inside a block a `)` closes it even in `echo`'s text.
            vec![("echo", Keyword), (")", Punctuation)],
            vec![(")", Punctuation)],
            // Outside one it is text.
            vec![("echo", Keyword)],
        ]
    );
    let state = lex_line(b"if 1==1 ((", LineState::default(), None);
    assert_eq!(
        state,
        LineState {
            depth: 2,
            carry: None
        }
    );
    let state = lex_line(b")", state, None);
    assert_eq!(state.depth, 1);
}

#[test]
fn a_caret_at_the_end_goes_on_with_the_next_line() {
    assert_eq!(
        lines("echo one ^\ntwo & dir ^\n  /a\nset x=^\nif\necho a ^^\nif"),
        vec![
            vec![("echo", Keyword)],
            vec![("&", Operator)],
            // An argument of `dir`, not a command.
            vec![("/a", Keyword)],
            vec![("set", Keyword)],
            vec![],
            // `^^` is a caret, not a continuation.
            vec![("echo", Keyword)],
            vec![("if", Keyword)],
        ]
    );
    // The caret escapes the first character of the next line.
    assert_eq!(
        lines("echo a ^\n& b"),
        vec![vec![("echo", Keyword)], vec![]]
    );
    assert_eq!(
        lex_line(b"echo \"a ^", LineState::default(), None).carry,
        None,
        "a caret in quotes is text"
    );
    assert_eq!(
        lex_line(b"echo a ^", LineState::default(), None).carry,
        Some(Mode::Echo { quoted: false })
    );
}

#[test]
fn crlf_lines_keep_their_carriage_return_out_of_the_tokens() {
    assert_eq!(
        lines("@echo off\r\nrem x\r\ngoto :eof\r\n"),
        vec![
            vec![("@", Punctuation), ("echo", Keyword)],
            vec![("rem x", Comment)],
            vec![("goto", Keyword), (":eof", Function)],
            vec![],
        ]
    );
}

#[test]
fn tokens_sit_on_character_boundaries() {
    let line = "echo Gr\u{fc}\u{df}e %\u{e4}% \u{fc}!x!\u{e4} & set \u{e4}=1 \"\u{fc}\"";
    assert_eq!(
        one(line),
        vec![
            ("echo", Keyword),
            ("%\u{e4}%", Key),
            ("!x!", Key),
            ("&", Operator),
            ("set", Keyword),
            ("1", Number),
            ("\"\u{fc}\"", STRING),
        ]
    );
}

#[test]
fn a_line_gets_at_most_4096_tokens() {
    let line = format!("echo {}", "%a%".repeat(5000));
    let mut tokens = Vec::new();
    lex_line(line.as_bytes(), LineState::default(), Some(&mut tokens));
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
    assert!(
        tokens
            .windows(2)
            .all(|pair| pair[0].start + pair[0].len <= pair[1].start)
    );
}
