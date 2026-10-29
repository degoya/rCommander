use super::*;
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, TypeName,
};

const STRING: TokenClass = TokenClass::String;

/// The tokens of each line of `text`, as the text they cover, lexed in order from the start.
fn lines(text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(line.as_bytes(), state, &mut tokens);
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
fn keywords_in_any_case_where_a_statement_starts() {
    for keyword in [
        "if", "elseif", "else", "switch", "foreach", "for", "while", "do", "until", "function",
        "filter", "param", "begin", "process", "end", "return", "break", "continue", "try",
        "catch", "finally", "throw", "trap", "class", "enum", "using", "exit",
    ] {
        assert_eq!(one(keyword), vec![(keyword, Keyword)], "{keyword}");
        let upper = keyword.to_ascii_uppercase();
        assert_eq!(one(&upper), vec![(upper.as_str(), Keyword)], "{upper}");
    }
    assert_eq!(
        one("if ($x -eq 1) { return $true } elseif ($y) { break } else { exit }"),
        vec![
            ("if", Keyword),
            ("(", Punctuation),
            ("$x", Key),
            ("-eq", Operator),
            ("1", Number),
            (")", Punctuation),
            ("{", Punctuation),
            ("return", Keyword),
            ("$true", Literal),
            ("}", Punctuation),
            ("elseif", Keyword),
            ("(", Punctuation),
            ("$y", Key),
            (")", Punctuation),
            ("{", Punctuation),
            ("break", Keyword),
            ("}", Punctuation),
            ("else", Keyword),
            ("{", Punctuation),
            ("exit", Keyword),
            ("}", Punctuation),
        ]
    );
    // An argument that spells a keyword is an argument.
    assert_eq!(one("Write-Output if"), vec![("Write-Output", Function)]);
}

#[test]
fn cmdlets_parameters_and_the_names_of_definitions_are_coloured() {
    assert_eq!(
        one("Get-ChildItem -Path C:\\temp -Recurse | Where-Object { $_.Length -gt 1kb }"),
        vec![
            ("Get-ChildItem", Function),
            ("-Path", Operator),
            ("-Recurse", Operator),
            ("|", Operator),
            ("Where-Object", Function),
            ("{", Punctuation),
            ("$_", Key),
            ("-gt", Operator),
            ("1kb", Number),
            ("}", Punctuation),
        ]
    );
    assert_eq!(
        one("function Get-Thing { param([string]$Name) }"),
        vec![
            ("function", Keyword),
            ("Get-Thing", Function),
            ("{", Punctuation),
            ("param", Keyword),
            ("(", Punctuation),
            ("[", Punctuation),
            ("string", TypeName),
            ("]", Punctuation),
            ("$Name", Key),
            (")", Punctuation),
            ("}", Punctuation),
        ]
    );
    assert_eq!(
        one("filter prompt {}"),
        vec![
            ("filter", Keyword),
            ("prompt", Function),
            ("{", Punctuation),
            ("}", Punctuation),
        ]
    );
    assert_eq!(
        one("class Shape : Base {}"),
        vec![
            ("class", Keyword),
            ("Shape", TypeName),
            ("{", Punctuation),
            ("}", Punctuation),
        ]
    );
    // Another command's name, an alias or a path, is not coloured; its arguments neither.
    assert_eq!(one("git status -s"), vec![("-s", Operator)]);
    assert_eq!(
        one(". .\\lib.ps1; & $exe 5"),
        vec![
            (".", Operator),
            (";", Operator),
            ("&", Operator),
            ("$exe", Key),
            ("5", Number),
        ]
    );
}

#[test]
fn variables_are_keys_and_true_false_null_literals() {
    assert_eq!(
        one("$env:PATH = \"$script:x ${a b} $_\""),
        vec![
            ("$env:PATH", Key),
            ("=", Operator),
            ("\"", STRING),
            ("$script:x", Key),
            (" ", STRING),
            ("${a b}", Key),
            (" ", STRING),
            ("$_", Key),
            ("\"", STRING),
        ]
    );
    assert_eq!(
        one("$true; $FALSE; $null; $trueish"),
        vec![
            ("$true", Literal),
            (";", Operator),
            ("$FALSE", Literal),
            (";", Operator),
            ("$null", Literal),
            (";", Operator),
            ("$trueish", Key),
        ]
    );
}

#[test]
fn types_attributes_and_indexes() {
    assert_eq!(
        one("[CmdletBinding()]"),
        vec![
            ("[", Punctuation),
            ("CmdletBinding", TypeName),
            ("(", Punctuation),
            (")", Punctuation),
            ("]", Punctuation),
        ]
    );
    assert_eq!(
        one("[Parameter(Mandatory = $true)]"),
        vec![
            ("[", Punctuation),
            ("Parameter", TypeName),
            ("(", Punctuation),
            ("=", Operator),
            ("$true", Literal),
            (")", Punctuation),
            ("]", Punctuation),
        ]
    );
    assert_eq!(
        one("[System.IO.File]::ReadAllText($p)"),
        vec![
            ("[", Punctuation),
            ("System.IO.File", TypeName),
            ("]", Punctuation),
            ("::", Operator),
            ("(", Punctuation),
            ("$p", Key),
            (")", Punctuation),
        ]
    );
    assert_eq!(
        one("[int[]]$a = 1, 2"),
        vec![
            ("[", Punctuation),
            ("int", TypeName),
            ("[", Punctuation),
            ("]", Punctuation),
            ("]", Punctuation),
            ("$a", Key),
            ("=", Operator),
            ("1", Number),
            (",", Punctuation),
            ("2", Number),
        ]
    );
    assert_eq!(
        one("$a[0]"),
        vec![
            ("$a", Key),
            ("[", Punctuation),
            ("0", Number),
            ("]", Punctuation),
        ]
    );
}

#[test]
fn operators_redirections_and_numbers() {
    assert_eq!(
        one("Get-Item x 2>&1 > $null; $a += 1; 1..10 && $b || $c"),
        vec![
            ("Get-Item", Function),
            ("2>&1", Operator),
            (">", Operator),
            ("$null", Literal),
            (";", Operator),
            ("$a", Key),
            ("+=", Operator),
            ("1", Number),
            (";", Operator),
            ("1", Number),
            ("..", Operator),
            ("10", Number),
            ("&&", Operator),
            ("$b", Key),
            ("||", Operator),
            ("$c", Key),
        ]
    );
    assert_eq!(
        one("$n = 0x1F + 1.5e3 - .5 * 10MB / 42l"),
        vec![
            ("$n", Key),
            ("=", Operator),
            ("0x1F", Number),
            ("+", Operator),
            ("1.5e3", Number),
            ("-", Operator),
            (".5", Number),
            ("*", Operator),
            ("10MB", Number),
            ("/", Operator),
            ("42l", Number),
        ]
    );
    assert_eq!(
        one("-not $a -and !$b"),
        vec![
            ("-not", Operator),
            ("$a", Key),
            ("-and", Operator),
            ("!", Operator),
            ("$b", Key),
        ]
    );
    assert_eq!(one("7zip a x.7z"), vec![]);
}

#[test]
fn the_format_operator_and_splatting() {
    assert_eq!(
        one("'{0} of {1}' -f $a, $b"),
        vec![
            ("'{0} of {1}'", STRING),
            ("-f", Operator),
            ("$a", Key),
            (",", Punctuation),
            ("$b", Key),
        ]
    );
    assert_eq!(
        one("Copy-Item @args -Force"),
        vec![
            ("Copy-Item", Function),
            ("@args", Key),
            ("-Force", Operator)
        ]
    );
    assert_eq!(
        one("$p = @{ Path = 'x'; Count = 2 }"),
        vec![
            ("$p", Key),
            ("=", Operator),
            ("@{", Punctuation),
            ("=", Operator),
            ("'x'", STRING),
            (";", Operator),
            ("=", Operator),
            ("2", Number),
            ("}", Punctuation),
        ]
    );
}

#[test]
fn strings_quote_their_quotes_and_expand_in_double_quotes() {
    assert_eq!(one("'it''s $x'"), vec![("'it''s $x'", STRING)]);
    assert_eq!(
        one("\"say \"\"hi\"\" `$x `\"\" $y"),
        vec![("\"say \"\"hi\"\" `$x `\"\"", STRING), ("$y", Key)]
    );
    assert_eq!(one("\"cost: $ 5\""), vec![("\"cost: $ 5\"", STRING)]);
    assert_eq!(
        one("\"$($a.b + \"$c\")\""),
        vec![
            ("\"", STRING),
            ("$(", Punctuation),
            ("$a", Key),
            ("+", Operator),
            ("\"", STRING),
            ("$c", Key),
            ("\"", STRING),
            (")", Punctuation),
            ("\"", STRING),
        ]
    );
    assert_eq!(
        one("Write-Host \"a $((1 + 2)) b\" -Fore Red"),
        vec![
            ("Write-Host", Function),
            ("\"a ", STRING),
            ("$(", Punctuation),
            ("(", Punctuation),
            ("1", Number),
            ("+", Operator),
            ("2", Number),
            (")", Punctuation),
            (")", Punctuation),
            (" b\"", STRING),
            ("-Fore", Operator),
        ]
    );
}

#[test]
fn strings_go_on_over_lines() {
    assert_eq!(
        lines("$s = 'one\ntwo' + \"three\nfour $x\"; Get-Date"),
        vec![
            vec![("$s", Key), ("=", Operator), ("'one", STRING)],
            vec![("two'", STRING), ("+", Operator), ("\"three", STRING),],
            vec![
                ("four ", STRING),
                ("$x", Key),
                ("\"", STRING),
                (";", Operator),
                ("Get-Date", Function),
            ],
        ]
    );
}

#[test]
fn here_strings_of_both_kinds() {
    let text =
        "$h = @\"\nHello $name \"quoted\"\n$(Get-Date)\n\"@\n$l = @'\n$not a var\n'@ | Out-Null";
    assert_eq!(
        lines(text),
        vec![
            vec![("$h", Key), ("=", Operator), ("@\"", STRING)],
            vec![("Hello ", STRING), ("$name", Key), (" \"quoted\"", STRING),],
            vec![
                ("$(", Punctuation),
                ("Get-Date", Function),
                (")", Punctuation),
            ],
            vec![("\"@", STRING)],
            vec![("$l", Key), ("=", Operator), ("@'", STRING)],
            vec![("$not a var", STRING)],
            vec![("'@", STRING), ("|", Operator), ("Out-Null", Function)],
        ]
    );
    // The closer stands at the start of a line; text after `@"` makes no here-string.
    assert_eq!(
        lines("@'\n  '@\n'@"),
        vec![
            vec![("@'", STRING)],
            vec![("  '@", STRING)],
            vec![("'@", STRING)],
        ]
    );
    assert_eq!(one("@\"x\""), vec![("@", Punctuation), ("\"x\"", STRING)]);
}

#[test]
fn comments_to_the_line_end_and_block_comments_over_lines() {
    assert_eq!(
        one("Get-Item x # a comment"),
        vec![("Get-Item", Function), ("# a comment", Comment)]
    );
    assert_eq!(
        lines("<#\n.SYNOPSIS\n    Does things.\n#>\nfunction Get-X {}"),
        vec![
            vec![("<#", Comment)],
            vec![(".SYNOPSIS", Comment)],
            vec![("    Does things.", Comment)],
            vec![("#>", Comment)],
            vec![
                ("function", Keyword),
                ("Get-X", Function),
                ("{", Punctuation),
                ("}", Punctuation),
            ],
        ]
    );
    assert_eq!(
        one("<# a #> Get-X a#b"),
        vec![("<# a #>", Comment), ("Get-X", Function)]
    );
}

#[test]
fn a_backtick_continues_the_command_or_escapes_a_character() {
    assert_eq!(
        lines("Write-Output `\n  Get-Date"),
        vec![vec![("Write-Output", Function)], vec![]]
    );
    assert_eq!(
        lines("Write-Output\n  Get-Date"),
        vec![
            vec![("Write-Output", Function)],
            vec![("Get-Date", Function)]
        ]
    );
    assert_eq!(
        one("Write-Output a`$b `|"),
        vec![("Write-Output", Function)]
    );
}

#[test]
fn the_state_at_a_line_end_is_all_the_next_line_needs() {
    let mut state = LineState::default();
    for line in ["$x = \"a $(", "  Get-Date"] {
        state = lex_line(line.as_bytes(), state, &mut ());
    }
    // Inside `$(` inside a string: a `)` and a `"` close both.
    let mut tokens: Vec<Token> = Vec::new();
    let after = lex_line(b")\"; 1", state, &mut tokens);
    assert_eq!(after, LineState::default());
    let classes: Vec<_> = tokens.iter().map(|token| token.class).collect();
    assert_eq!(classes, vec![Punctuation, STRING, Operator, Number]);
}

#[test]
fn a_line_gets_at_most_4096_tokens() {
    let line = format!("Write-Output {}", "$a ".repeat(5000));
    let mut tokens: Vec<Token> = Vec::new();
    lex_line(line.as_bytes(), LineState::default(), &mut tokens);
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
}

#[test]
fn opens_remember_the_strings_left_open() {
    let mut opens = Opens::default();
    let mut state = LineState::default();
    for line in [
        "$a = 'closed' + \"open",
        "still\"; $h = @'",
        "'@ + \"x\" + 'y",
    ] {
        state = lex_line(line.as_bytes(), state, &mut opens);
        opens.line += 1;
    }
    assert_eq!(
        opens.into_unclosed(),
        vec![QuoteOpen {
            line: 2,
            col: 11,
            width: 1
        }]
    );
}
