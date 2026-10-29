use super::{closes, heredoc_open};
use crate::lexer::state::fnv;
use crate::test_support::{assert_plain, assert_tokens, highlighted};
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

const PHP: &str = concat!(
    "<?php\n",
    "namespace App\\Models;\n",
    "use Foo\\Bar;\n",
    "# hash comment\n",
    "/* block\n",
    "   comment */\n",
    "final class User extends Model implements HasName {\n",
    "    public function getName(int $id): ?string {\n",
    "        $x = \"Hello {$this->name} $id\\n\";\n",
    "        return self::format('%s $x', $x) . PHP_EOL ?? null;\n",
    "    }\n",
    "}\n",
    "#[Attr]\n",
    "$n = 0x1A + 1_000 + 1.5;\n",
    "echo TRUE, strlen($s), $obj->run();\n",
);

#[test]
fn php_has_its_classes() {
    let lines = highlighted("a.php", PHP.as_bytes());
    assert_eq!(lines[0], vec![(Punctuation, "<?php".to_owned())]);
    assert_tokens(
        &lines[1],
        &[(Keyword, "namespace"), (TypeName, "App\\Models")],
    );
    assert_tokens(&lines[2], &[(Keyword, "use"), (TypeName, "Foo\\Bar")]);
    assert_eq!(lines[3], vec![(Comment, "# hash comment".to_owned())]);
    assert_eq!(lines[4], vec![(Comment, "/* block".to_owned())]);
    assert_eq!(lines[5], vec![(Comment, "   comment */".to_owned())]);
    assert_tokens(
        &lines[6],
        &[
            (Keyword, "final"),
            (Keyword, "class"),
            (TypeName, "User"),
            (Keyword, "extends"),
            (TypeName, "Model"),
            (Keyword, "implements"),
            (TypeName, "HasName"),
        ],
    );
    assert_tokens(
        &lines[7],
        &[
            (Keyword, "public"),
            (Keyword, "function"),
            (Function, "getName"),
            (TypeName, "int"),
            (Key, "$id"),
            (TypeName, "string"),
        ],
    );
    assert_tokens(
        &lines[8],
        &[(Key, "$x"), (Operator, "="), (Key, "$this"), (Key, "$id")],
    );
    assert_tokens(
        &lines[9],
        &[
            (Keyword, "return"),
            (TypeName, "self"),
            (Operator, "::"),
            (Function, "format"),
            (String, "'%s $x'"),
            (Key, "$x"),
            (Literal, "null"),
        ],
    );
    // A constant in capitals stays plain.
    assert_plain(&lines[9], "PHP_EOL");
    assert_tokens(
        &lines[12],
        &[(Punctuation, "#["), (TypeName, "Attr"), (Punctuation, "]")],
    );
    assert_tokens(
        &lines[13],
        &[(Number, "0x1A"), (Number, "1_000"), (Number, "1.5")],
    );
    assert_tokens(
        &lines[14],
        &[
            (Keyword, "echo"),
            (Literal, "TRUE"),
            (Function, "strlen"),
            (Operator, "->"),
            (Function, "run"),
        ],
    );
}

#[test]
fn a_string_goes_over_lines_with_its_variables() {
    let lines = highlighted("a.php", b"<?php\n$s = \"one\ntwo $x\";\n$t = 'a\nb';\n");
    assert_tokens(&lines[1], &[(Key, "$s"), (String, "\"one")]);
    assert_eq!(
        lines[2],
        vec![
            (String, "two ".to_owned()),
            (Key, "$x".to_owned()),
            (String, "\"".to_owned()),
            (Punctuation, ";".to_owned()),
        ]
    );
    assert_tokens(&lines[3], &[(String, "'a")]);
    assert_tokens(&lines[4], &[(String, "b'")]);
}

#[test]
fn a_heredoc_and_a_nowdoc() {
    let text = concat!(
        "<?php\n",
        "$t = <<<EOT\n",
        "  Hi $name\n",
        "  EOT;\n",
        "$u = <<<'RAW'\n",
        "$kept as text\n",
        "RAW;\n",
    );
    let lines = highlighted("a.php", text.as_bytes());
    assert_tokens(&lines[1], &[(Keyword, "<<<EOT")]);
    assert_eq!(
        lines[2],
        vec![(String, "  Hi ".to_owned()), (Key, "$name".to_owned())]
    );
    assert_eq!(
        lines[3],
        vec![(Keyword, "EOT".to_owned()), (Punctuation, ";".to_owned())]
    );
    assert_tokens(&lines[4], &[(Keyword, "<<<'RAW'")]);
    assert_eq!(lines[5], vec![(String, "$kept as text".to_owned())]);
    assert_eq!(
        lines[6],
        vec![(Keyword, "RAW".to_owned()), (Punctuation, ";".to_owned())]
    );
}

#[test]
fn heredoc_openers_and_markers() {
    let (len, marker) = heredoc_open(b"<<<EOT\n").expect("an opener");
    assert_eq!((len, marker.len, marker.nowdoc), (6, 3, false));
    assert_eq!(marker.hash, fnv(b"EOT"));
    let (len, marker) = heredoc_open(b"<<< \"A_1\"\n").expect("an opener");
    assert_eq!((len, marker.len, marker.nowdoc), (9, 3, false));
    let (len, marker) = heredoc_open(b"<<<'N'\n").expect("an opener");
    assert_eq!((len, marker.nowdoc), (6, true));
    assert!(heredoc_open(b"<<<1x").is_none());
    assert!(heredoc_open(b"<<<'open\n").is_none());
    assert_eq!(closes(b"  EOT;", marker_of(b"EOT")), Some(2));
    assert_eq!(closes(b"EOTX;", marker_of(b"EOT")), None);
}

fn marker_of(name: &[u8]) -> crate::lexer::state::Marker {
    heredoc_open(&[&b"<<<"[..], name, &b"\n"[..]].concat())
        .expect("an opener")
        .1
}

#[test]
fn a_line_comment_ends_before_the_closing_tag() {
    let lines = highlighted("a.php", b"<?php // note ?><b>x</b>\n");
    assert_eq!(
        lines[0],
        vec![
            (Punctuation, "<?php".to_owned()),
            (Comment, "// note ".to_owned()),
            (Punctuation, "?>".to_owned()),
            (Punctuation, "<".to_owned()),
            (TypeName, "b".to_owned()),
            (Punctuation, ">".to_owned()),
            (Punctuation, "</".to_owned()),
            (TypeName, "b".to_owned()),
            (Punctuation, ">".to_owned()),
        ]
    );
}

#[test]
fn a_closing_tag_in_a_string_or_a_block_comment_ends_nothing() {
    let lines = highlighted("a.php", b"<?php $a = '?>'; /* ?> */ $b;\n");
    assert_tokens(
        &lines[0],
        &[(String, "'?>'"), (Comment, "/* ?> */"), (Key, "$b")],
    );
}
