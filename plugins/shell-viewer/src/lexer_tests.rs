use crate::test_support::{highlighted, line};
use crate::types::TokenClass;

use TokenClass as C;

/// `(class, text)` pairs as `line` and `highlighted` answer them.
fn spans(list: &[(TokenClass, &str)]) -> Vec<(TokenClass, String)> {
    list.iter()
        .map(|(class, text)| (*class, (*text).to_owned()))
        .collect()
}

fn lines(text: &str) -> Vec<Vec<(TokenClass, String)>> {
    highlighted(text.as_bytes())
}

#[test]
fn keywords_and_builtins_at_a_command_start() {
    assert_eq!(
        line("if true; then echo hi; fi"),
        spans(&[
            (C::Keyword, "if"),
            (C::Function, "true"),
            (C::Operator, ";"),
            (C::Keyword, "then"),
            (C::Function, "echo"),
            (C::Operator, ";"),
            (C::Keyword, "fi"),
        ])
    );
    // An argument is not a command: `done` and `echo` stay plain there.
    assert_eq!(line("echo done echo"), spans(&[(C::Function, "echo")]));
    assert_eq!(
        line("while [[ -n $x ]]; do :; done"),
        spans(&[
            (C::Keyword, "while"),
            (C::Keyword, "[["),
            (C::Key, "$x"),
            (C::Keyword, "]]"),
            (C::Operator, ";"),
            (C::Keyword, "do"),
            (C::Function, ":"),
            (C::Operator, ";"),
            (C::Keyword, "done"),
        ])
    );
    // A command of no colour.
    assert_eq!(line("ls -la /tmp"), Vec::new());
}

#[test]
fn a_function_definition_names_a_function() {
    let got = lines("deploy() {\n  return 0\n}\nfunction build {\n}\nname () { :; }\n");
    assert_eq!(
        got[0],
        spans(&[
            (C::Function, "deploy"),
            (C::Punctuation, "("),
            (C::Punctuation, ")"),
            (C::Punctuation, "{"),
        ])
    );
    assert_eq!(got[1], spans(&[(C::Function, "return"), (C::Number, "0")]));
    assert_eq!(got[2], spans(&[(C::Punctuation, "}")]));
    assert_eq!(
        got[3],
        spans(&[
            (C::Keyword, "function"),
            (C::Function, "build"),
            (C::Punctuation, "{"),
        ])
    );
    assert_eq!(got[4], spans(&[(C::Punctuation, "}")]));
    assert_eq!(got[5][0], (C::Function, "name".to_owned()));
}

#[test]
fn variables_and_assignments_are_keys() {
    assert_eq!(
        line("echo $x ${y} $1 $@ $? $$ $#"),
        spans(&[
            (C::Function, "echo"),
            (C::Key, "$x"),
            (C::Key, "${y}"),
            (C::Key, "$1"),
            (C::Key, "$@"),
            (C::Key, "$?"),
            (C::Key, "$$"),
            (C::Key, "$#"),
        ])
    );
    assert_eq!(
        line("FOO=1 BAR+=x arr[2]=y cmd"),
        spans(&[
            (C::Key, "FOO"),
            (C::Operator, "="),
            (C::Number, "1"),
            (C::Key, "BAR"),
            (C::Operator, "+="),
            (C::Key, "arr[2]"),
            (C::Operator, "="),
        ])
    );
    assert_eq!(
        line("local name=\"v\""),
        spans(&[
            (C::Function, "local"),
            (C::Key, "name"),
            (C::Operator, "="),
            (C::String, "\"v\""),
        ])
    );
    // Not at a command's start, `a=b` is an argument.
    assert_eq!(line("echo a=b"), spans(&[(C::Function, "echo")]));
    assert_eq!(
        line("for i in 1 2; do"),
        spans(&[
            (C::Keyword, "for"),
            (C::Key, "i"),
            (C::Keyword, "in"),
            (C::Number, "1"),
            (C::Number, "2"),
            (C::Operator, ";"),
            (C::Keyword, "do"),
        ])
    );
}

#[test]
fn strings_colour_their_expansions() {
    assert_eq!(
        line(r#"echo 'single $x' "double $v in" $'esc\'aped'"#),
        spans(&[
            (C::Function, "echo"),
            (C::String, "'single $x'"),
            (C::String, "\"double "),
            (C::Key, "$v"),
            (C::String, " in\""),
            (C::String, r"$'esc\'aped'"),
        ])
    );
    assert_eq!(
        line(r#"echo "a\"b $ c""#),
        spans(&[(C::Function, "echo"), (C::String, r#""a\"b $ c""#)])
    );
}

#[test]
fn nested_substitutions_inside_quotes() {
    assert_eq!(
        line(r#"x="$(basename "$(pwd)")""#),
        spans(&[
            (C::Key, "x"),
            (C::Operator, "="),
            (C::String, "\""),
            (C::Punctuation, "$("),
            (C::String, "\""),
            (C::Punctuation, "$("),
            (C::Function, "pwd"),
            (C::Punctuation, ")"),
            (C::String, "\""),
            (C::Punctuation, ")"),
            (C::String, "\""),
        ])
    );
    assert_eq!(
        line("echo `date` done"),
        spans(&[
            (C::Function, "echo"),
            (C::Punctuation, "`"),
            (C::Punctuation, "`"),
        ])
    );
}

#[test]
fn a_parameter_expansion_with_a_quoted_default() {
    assert_eq!(
        line(r#"echo ${x:-"a b"} ${#arr[@]}"#),
        spans(&[
            (C::Function, "echo"),
            (C::Key, "${x:-"),
            (C::String, "\"a b\""),
            (C::Key, "}"),
            (C::Key, "${#arr[@]}"),
        ])
    );
}

#[test]
fn a_hash_inside_a_word_starts_no_comment() {
    assert_eq!(
        line("echo a#b ${#x} $# # real"),
        spans(&[
            (C::Function, "echo"),
            (C::Key, "${#x}"),
            (C::Key, "$#"),
            (C::Comment, "# real"),
        ])
    );
    let got = lines("#!/bin/bash\n# a comment\necho x;# after\n");
    assert_eq!(got[0], spans(&[(C::Comment, "#!/bin/bash")]));
    assert_eq!(got[1], spans(&[(C::Comment, "# a comment")]));
    assert_eq!(
        got[2],
        spans(&[
            (C::Function, "echo"),
            (C::Operator, ";"),
            (C::Comment, "# after"),
        ])
    );
}

#[test]
fn heredocs_in_all_three_forms() {
    let got = lines(concat!(
        "cat <<EOF\n",
        "Hi $USER ${HOME}\n",
        "EOF\n",
        "cat <<-END\n",
        "\tindented\n",
        "\tEND\n",
        "cat <<'RAW'\n",
        "$HOME stays\n",
        "RAW\n",
        "echo done\n",
    ));
    assert_eq!(got[0], spans(&[(C::Operator, "<<"), (C::Keyword, "EOF")]));
    assert_eq!(
        got[1],
        spans(&[
            (C::String, "Hi "),
            (C::Key, "$USER"),
            (C::String, " "),
            (C::Key, "${HOME}"),
        ])
    );
    assert_eq!(got[2], spans(&[(C::Keyword, "EOF")]));
    assert_eq!(got[3], spans(&[(C::Operator, "<<-"), (C::Keyword, "END")]));
    assert_eq!(got[4], spans(&[(C::String, "\tindented")]));
    assert_eq!(got[5], spans(&[(C::Keyword, "END")]));
    assert_eq!(got[6], spans(&[(C::Operator, "<<"), (C::Keyword, "'RAW'")]));
    assert_eq!(got[7], spans(&[(C::String, "$HOME stays")]));
    assert_eq!(got[8], spans(&[(C::Keyword, "RAW")]));
    // After the heredoc, a command again; `done` is its argument.
    assert_eq!(got[9], spans(&[(C::Function, "echo")]));
}

#[test]
fn a_heredoc_inside_a_substitution_and_a_line_that_only_looks_like_the_marker() {
    let got = lines("x=$(cat <<EOF\nEOF2\n  EOF\nEOF\n)\n");
    assert_eq!(got[1], spans(&[(C::String, "EOF2")]));
    assert_eq!(got[2], spans(&[(C::String, "  EOF")]));
    assert_eq!(got[3], spans(&[(C::Keyword, "EOF")]));
    assert_eq!(got[4], spans(&[(C::Punctuation, ")")]));
}

#[test]
fn a_backslash_at_the_line_end_continues_the_command() {
    let got = lines("docker run \\\n  echo x\necho y\n");
    assert_eq!(got[0], Vec::new());
    // `echo` is an argument of `docker` here, not a command.
    assert_eq!(got[1], Vec::new());
    assert_eq!(got[2], spans(&[(C::Function, "echo")]));
}

#[test]
fn case_patterns_end_with_a_parenthesis() {
    let got = lines(concat!(
        "case \"$1\" in\n",
        "  start|run) echo go ;;\n",
        "  *) exit 2 ;;\n",
        "esac\n",
        "echo after\n",
    ));
    assert_eq!(
        got[0],
        spans(&[
            (C::Keyword, "case"),
            (C::String, "\""),
            (C::Key, "$1"),
            (C::String, "\""),
            (C::Keyword, "in"),
        ])
    );
    assert_eq!(
        got[1],
        spans(&[
            (C::Operator, "|"),
            (C::Punctuation, ")"),
            (C::Function, "echo"),
            (C::Operator, ";;"),
        ])
    );
    assert_eq!(
        got[2],
        spans(&[
            (C::Punctuation, ")"),
            (C::Function, "exit"),
            (C::Number, "2"),
            (C::Operator, ";;"),
        ])
    );
    assert_eq!(got[3], spans(&[(C::Keyword, "esac")]));
    assert_eq!(got[4], spans(&[(C::Function, "echo")]));
    // A pattern's `)` inside `$(` does not close the substitution.
    assert_eq!(
        line("x=$(case $a in y) echo 1;; esac)"),
        spans(&[
            (C::Key, "x"),
            (C::Operator, "="),
            (C::Punctuation, "$("),
            (C::Keyword, "case"),
            (C::Key, "$a"),
            (C::Keyword, "in"),
            (C::Punctuation, ")"),
            (C::Function, "echo"),
            (C::Number, "1"),
            (C::Operator, ";;"),
            (C::Keyword, "esac"),
            (C::Punctuation, ")"),
        ])
    );
}

#[test]
fn operators_and_redirections() {
    assert_eq!(
        line("a && b || c | d & e; f >> g 2>&1 < h <(i) &> j >&2"),
        spans(&[
            (C::Operator, "&&"),
            (C::Operator, "||"),
            (C::Operator, "|"),
            (C::Operator, "&"),
            (C::Operator, ";"),
            (C::Operator, ">>"),
            (C::Operator, "2>&1"),
            (C::Operator, "<"),
            (C::Operator, "<("),
            (C::Punctuation, ")"),
            (C::Operator, "&>"),
            (C::Operator, ">&2"),
        ])
    );
    // A redirection's target is not a command: `echo` stays plain after `>`.
    assert_eq!(
        line("cat <<< \"$x\" > echo"),
        spans(&[
            (C::Operator, "<<<"),
            (C::String, "\""),
            (C::Key, "$x"),
            (C::String, "\""),
            (C::Operator, ">"),
        ])
    );
}

#[test]
fn numbers_and_arithmetic() {
    assert_eq!(
        line("sleep 0.5; echo $(( (x + 16#ff) * 2 ))"),
        spans(&[
            (C::Number, "0.5"),
            (C::Operator, ";"),
            (C::Function, "echo"),
            (C::Punctuation, "$(("),
            (C::Punctuation, "("),
            (C::Key, "x"),
            (C::Operator, "+"),
            (C::Number, "16#ff"),
            (C::Punctuation, ")"),
            (C::Operator, "*"),
            (C::Number, "2"),
            (C::Punctuation, "))"),
        ])
    );
    assert_eq!(
        line("(( i++ ))"),
        spans(&[
            (C::Punctuation, "(("),
            (C::Key, "i"),
            (C::Operator, "++"),
            (C::Punctuation, "))"),
        ])
    );
}

#[test]
fn a_quote_open_at_the_end_of_the_file_is_invalid() {
    let got = lines("echo \"never closed\nmore text\n");
    assert_eq!(
        got[0],
        spans(&[
            (C::Function, "echo"),
            (C::Invalid, "\""),
            (C::String, "never closed"),
        ])
    );
    assert_eq!(got[1], spans(&[(C::String, "more text")]));
    assert_eq!(
        line("echo 'x"),
        spans(&[(C::Function, "echo"), (C::Invalid, "'"), (C::String, "x")])
    );
    // A quote closed on a later line is a string, not an error.
    let closed = lines("echo \"two\nlines\"\n");
    assert_eq!(closed[0][1], (C::String, "\"two".to_owned()));
    assert_eq!(closed[1], spans(&[(C::String, "lines\"")]));
}

#[test]
fn crlf_and_a_byte_order_mark() {
    let got = highlighted(b"\xEF\xBB\xBFecho \"a\" # c\r\nfi\r\n");
    assert_eq!(got.len(), 2);
    assert_eq!(
        got[0],
        spans(&[
            (C::Function, "echo"),
            (C::String, "\"a\""),
            (C::Comment, "# c"),
        ])
    );
    assert_eq!(got[1], spans(&[(C::Keyword, "fi")]));
    // A heredoc's marker line ends in `\r\n` too.
    let heredoc = highlighted(b"cat <<EOF\r\ntext\r\nEOF\r\necho\r\n");
    assert_eq!(heredoc[2], spans(&[(C::Keyword, "EOF")]));
    assert_eq!(heredoc[3], spans(&[(C::Function, "echo")]));
}

#[test]
fn bytes_that_are_not_utf8_count_as_the_replacement_character() {
    let got = highlighted(b"echo \"\xFF\xFE x\" $v \xC3\n");
    assert_eq!(
        got[0],
        spans(&[
            (C::Function, "echo"),
            (C::String, "\"\u{FFFD}\u{FFFD} x\""),
            (C::Key, "$v"),
        ])
    );
}
