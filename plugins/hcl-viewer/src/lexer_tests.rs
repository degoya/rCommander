//! Terraform and HCL samples, lexed line by line from the file's start.

use super::{Frame, LineState, SLOTS, lex_line};
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::Token;
use crate::types::TokenClass::{
    self, Comment, Function, Invalid, Key, Keyword, Literal, Number, Operator, Punctuation,
    TypeName,
};

const STRING: TokenClass = TokenClass::String;

#[test]
fn blocks_labels_attributes_and_interpolations() {
    let lines = lexed(concat!(
        "resource \"aws_instance\" \"web\" {\n",
        "  ami   = \"ami-123\"\n",
        "  count = 2\n",
        "  tags = {\n",
        "    Name = \"web-${var.env}\"\n",
        "  }\n",
        "  lifecycle {\n",
        "    ignore_changes = [tags]\n",
        "  }\n",
        "}",
    ));
    assert_eq!(
        lines[0],
        vec![
            ("resource", Keyword),
            ("\"aws_instance\"", STRING),
            ("\"web\"", STRING),
            ("{", Punctuation)
        ]
    );
    assert_eq!(
        lines[1],
        vec![("ami", Key), ("=", Operator), ("\"ami-123\"", STRING)]
    );
    assert_eq!(
        lines[2],
        vec![("count", Key), ("=", Operator), ("2", Number)]
    );
    assert_eq!(
        lines[3],
        vec![("tags", Key), ("=", Operator), ("{", Punctuation)]
    );
    // `var` names a value and `env` is its attribute: neither is coloured.
    assert_eq!(
        lines[4],
        vec![
            ("Name", Key),
            ("=", Operator),
            ("\"web-", STRING),
            ("${", Punctuation),
            (".", Punctuation),
            ("}", Punctuation),
            ("\"", STRING)
        ]
    );
    // A nested block's type is a keyword too.
    assert_eq!(lines[6], vec![("lifecycle", Keyword), ("{", Punctuation)]);
    assert_eq!(class_of(&lines[7..8], "ignore_changes"), Some(Key));
    assert_eq!(lines[9], vec![("}", Punctuation)]);
}

#[test]
fn only_a_line_s_first_word_before_labels_and_a_brace_heads_a_block() {
    let lines = lexed(concat!(
        "dynamic \"setting\" {\n",
        "legacy aws_instance web {\n",
        "locals {\n",
        "  merged = merge(\n",
        "    var.tags,\n",
        "    local.extra\n",
        "  )\n",
        "}",
    ));
    assert_eq!(lines[0][0], ("dynamic", Keyword));
    assert_eq!(lines[1][0], ("legacy", Keyword));
    assert_eq!(lines[2][0], ("locals", Keyword));
    assert_eq!(class_of(&lines[3..4], "merge"), Some(Function));
    // A value spread over lines is no block.
    assert!(texts_of(&lines[4..7], Keyword).is_empty());
    assert_eq!(lines[4], vec![(".", Punctuation), (",", Punctuation)]);
}

#[test]
fn comments_of_every_kind() {
    let lines = lexed(concat!(
        "# hash\n",
        "// slashes\n",
        "a = 1 /* starts\n",
        "   goes on\n",
        "ends */ b = true # after",
    ));
    assert_eq!(lines[0], vec![("# hash", Comment)]);
    assert_eq!(lines[1], vec![("// slashes", Comment)]);
    assert_eq!(lines[2].last(), Some(&("/* starts", Comment)));
    assert_eq!(lines[3], vec![("   goes on", Comment)]);
    assert_eq!(
        lines[4],
        vec![
            ("ends */", Comment),
            ("b", Key),
            ("=", Operator),
            ("true", Literal),
            ("# after", Comment)
        ]
    );
}

#[test]
fn heredocs_run_to_their_marker_with_interpolations_inside() {
    let lines = lexed(concat!(
        "user_data = <<-EOT\n",
        "  echo \"${var.name}\" $${literal}\n",
        "  EOT2 is no marker\n",
        "  EOT\n",
        "after = null",
    ));
    assert_eq!(
        lines[0],
        vec![
            ("user_data", Key),
            ("=", Operator),
            ("<<-", Operator),
            ("EOT", Keyword)
        ]
    );
    assert_eq!(
        lines[1],
        vec![
            ("  echo \"", STRING),
            ("${", Punctuation),
            (".", Punctuation),
            ("}", Punctuation),
            ("\" $${literal}", STRING)
        ]
    );
    assert_eq!(lines[2], vec![("  EOT2 is no marker", STRING)]);
    assert_eq!(lines[3], vec![("EOT", Keyword)]);
    assert_eq!(
        lines[4],
        vec![("after", Key), ("=", Operator), ("null", Literal)]
    );
}

#[test]
fn a_heredoc_needs_its_marker_alone_at_the_line_s_end() {
    let lines = lexed("x = <<EOT more\ny = 1");
    assert_eq!(
        lines[0],
        vec![
            ("x", Key),
            ("=", Operator),
            ("<", Operator),
            ("<", Operator)
        ]
    );
    assert_eq!(lines[1], vec![("y", Key), ("=", Operator), ("1", Number)]);
}

#[test]
fn template_directives_strip_markers_and_their_keywords() {
    let lines = lexed(concat!(
        "out = <<EOT\n",
        "%{ for ip in ips ~}\n",
        "server ${ip}\n",
        "%{ endfor }\n",
        "EOT",
    ));
    assert_eq!(
        lines[1],
        vec![
            ("%{", Punctuation),
            ("for", Keyword),
            ("in", Keyword),
            ("~}", Punctuation)
        ]
    );
    assert_eq!(
        lines[2],
        vec![("server ", STRING), ("${", Punctuation), ("}", Punctuation)]
    );
    assert_eq!(
        lines[3],
        vec![("%{", Punctuation), ("endfor", Keyword), ("}", Punctuation)]
    );
    assert_eq!(lines[4], vec![("EOT", Keyword)]);
    let lines = lexed("s = \"${~ x ~}\"");
    assert_eq!(
        lines[0],
        vec![
            ("s", Key),
            ("=", Operator),
            ("\"", STRING),
            ("${~", Punctuation),
            ("~}", Punctuation),
            ("\"", STRING)
        ]
    );
}

#[test]
fn for_expressions_functions_and_type_constraints() {
    let lines = lexed(concat!(
        "upper = [for s in var.list : upper(s) if s != \"\"]\n",
        "type = list(object({ name = string }))\n",
        "pairs = {for k, v in var.map : k => v}\n",
        "n = -1.5e3",
    ));
    assert_eq!(
        lines[0],
        vec![
            ("upper", Key),
            ("=", Operator),
            ("[", Punctuation),
            ("for", Keyword),
            ("in", Keyword),
            (".", Punctuation),
            (":", Operator),
            ("upper", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("if", Keyword),
            ("!=", Operator),
            ("\"\"", STRING),
            ("]", Punctuation)
        ]
    );
    assert_eq!(
        lines[1],
        vec![
            ("type", Key),
            ("=", Operator),
            ("list", TypeName),
            ("(", Punctuation),
            ("object", TypeName),
            ("(", Punctuation),
            ("{", Punctuation),
            ("name", Key),
            ("=", Operator),
            ("string", TypeName),
            ("}", Punctuation),
            (")", Punctuation),
            (")", Punctuation)
        ]
    );
    // `=>` is no assignment: `k` before it stays a name.
    assert_eq!(class_of(&lines[2..3], "=>"), Some(Operator));
    assert_eq!(class_of(&lines[2..3], "k"), None);
    assert_eq!(
        lines[3],
        vec![
            ("n", Key),
            ("=", Operator),
            ("-", Operator),
            ("1.5e3", Number)
        ]
    );
}

#[test]
fn templates_nest_in_sequences_and_escapes_stay_in_the_string() {
    let lines = lexed(concat!(
        "a = \"x ${join(\",\", [for k in keys : \"${k}\"])} y\"\n",
        "b = \"say \\\"hi\\\" ${upper(\"}\")}\"",
    ));
    assert_eq!(
        lines[0],
        vec![
            ("a", Key),
            ("=", Operator),
            ("\"x ", STRING),
            ("${", Punctuation),
            ("join", Function),
            ("(", Punctuation),
            ("\",\"", STRING),
            (",", Punctuation),
            ("[", Punctuation),
            ("for", Keyword),
            ("in", Keyword),
            (":", Operator),
            ("\"", STRING),
            ("${", Punctuation),
            ("}", Punctuation),
            ("\"", STRING),
            ("]", Punctuation),
            (")", Punctuation),
            ("}", Punctuation),
            (" y\"", STRING)
        ]
    );
    // A `}` in a string inside the sequence closes nothing.
    assert_eq!(
        lines[1],
        vec![
            ("b", Key),
            ("=", Operator),
            ("\"say \\\"hi\\\" ", STRING),
            ("${", Punctuation),
            ("upper", Function),
            ("(", Punctuation),
            ("\"}\"", STRING),
            (")", Punctuation),
            ("}", Punctuation),
            ("\"", STRING)
        ]
    );
}

#[test]
fn a_quoted_template_its_line_leaves_open_shows_its_quote_as_invalid() {
    let lines = lexed("name = \"open\nx = 1");
    assert_eq!(
        lines[0],
        vec![
            ("name", Key),
            ("=", Operator),
            ("\"", Invalid),
            ("open", STRING)
        ]
    );
    assert_eq!(lines[1], vec![("x", Key), ("=", Operator), ("1", Number)]);
}

#[test]
fn a_sequence_may_go_on_over_lines() {
    let lines = lexed("v = \"a ${merge(\n  x) } b\"\nw = 2");
    assert_eq!(class_of(&lines[0..1], "merge"), Some(Function));
    assert_eq!(
        lines[1],
        vec![(")", Punctuation), ("}", Punctuation), (" b\"", STRING)]
    );
    assert_eq!(lines[2], vec![("w", Key), ("=", Operator), ("2", Number)]);
}

#[test]
fn deep_nesting_stays_within_the_stack_and_the_line() {
    let deep = format!(
        "x = {}{}",
        "\"${".repeat(SLOTS + 4),
        "}\"".repeat(SLOTS + 4)
    );
    let mut tokens: Vec<Token> = Vec::new();
    let state = lex_line(deep.as_bytes(), LineState::default(), &mut tokens);
    assert!(tokens.iter().all(|token| {
        let end = token.start as usize + token.len as usize;
        end <= deep.len()
    }));
    assert!(state.frames.depth() <= SLOTS);
    // The next line starts in a sequence or in code, never in a quoted template.
    assert_ne!(state.frames.top(), Some(Frame::Quoted));
}
