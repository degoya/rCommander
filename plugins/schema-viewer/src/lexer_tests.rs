//! A Protobuf and a GraphQL sample with what sets each apart, lexed line by line from the file's
//! start.

use super::{LineState, Open, lex_line};
use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::Token;
use crate::types::TokenClass::{
    self, Comment, Function, Invalid, Key, Keyword, Literal, Number, Operator, Punctuation,
    TypeName,
};

const STRING: TokenClass = TokenClass::String;

const PROTO: &str = concat!(
    "syntax = \"proto3\";\n",
    "package acme.v1;\n",
    "import \"google/protobuf/timestamp.proto\";\n",
    "/* A user,\n",
    "   stored. */ message User {\n",
    "  string name = 1;\n",
    "  repeated .acme.v1.Tag tags = 2 [deprecated = true];\n",
    "  map<string, Project> projects = 3;\n",
    "  google.protobuf.Timestamp created = 4; // when\n",
    "  reserved 9 to 11, 15;\n",
    "  enum Kind { KIND_UNSPECIFIED = 0; ADMIN = -1; }\n",
    "}\n",
    "service Users {\n",
    "  rpc Get(GetRequest) returns (stream User) {\n",
    "    option (google.api.http) = { get: \"/v1/{name=users/*}\" };\n",
    "  }\n",
    "}",
);

#[test]
fn proto_declarations_fields_and_comments() {
    let lines = lexed(Lang::Proto, PROTO);
    assert_eq!(
        lines[0],
        vec![
            ("syntax", Keyword),
            ("=", Operator),
            ("\"proto3\"", STRING),
            (";", Punctuation)
        ]
    );
    // A package's dotted name is no type.
    assert_eq!(lines[1], vec![("package", Keyword), (";", Punctuation)]);
    assert_eq!(class_of(&lines[2..3], "import"), Some(Keyword));
    assert_eq!(lines[3], vec![("/* A user,", Comment)]);
    assert_eq!(
        lines[4],
        vec![
            ("   stored. */", Comment),
            ("message", Keyword),
            ("User", TypeName),
            ("{", Punctuation)
        ]
    );
    assert_eq!(
        lines[5],
        vec![
            ("string", TypeName),
            ("name", Key),
            ("=", Operator),
            ("1", Number),
            (";", Punctuation)
        ]
    );
    assert_eq!(
        lines[6],
        vec![
            ("repeated", Keyword),
            (".", Punctuation),
            ("acme.v1.Tag", TypeName),
            ("tags", Key),
            ("=", Operator),
            ("2", Number),
            ("[", Punctuation),
            ("deprecated", Key),
            ("=", Operator),
            ("true", Literal),
            ("]", Punctuation),
            (";", Punctuation)
        ]
    );
    assert_eq!(
        lines[7],
        vec![
            ("map", Keyword),
            ("<", Punctuation),
            ("string", TypeName),
            (",", Punctuation),
            ("Project", TypeName),
            (">", Punctuation),
            ("projects", Key),
            ("=", Operator),
            ("3", Number),
            (";", Punctuation)
        ]
    );
    assert_eq!(lines[8][0], ("google.protobuf.Timestamp", TypeName));
    assert_eq!(lines[8].last(), Some(&("// when", Comment)));
    assert_eq!(class_of(&lines[9..10], "to"), Some(Keyword));
    assert_eq!(
        lines[10],
        vec![
            ("enum", Keyword),
            ("Kind", TypeName),
            ("{", Punctuation),
            ("KIND_UNSPECIFIED", Key),
            ("=", Operator),
            ("0", Number),
            (";", Punctuation),
            ("ADMIN", Key),
            ("=", Operator),
            ("-", Operator),
            ("1", Number),
            (";", Punctuation),
            ("}", Punctuation)
        ]
    );
}

#[test]
fn proto_services_rpcs_and_aggregate_options() {
    let lines = lexed(Lang::Proto, PROTO);
    assert_eq!(lines[12][1], ("Users", TypeName));
    assert_eq!(
        lines[13],
        vec![
            ("rpc", Keyword),
            ("Get", Function),
            ("(", Punctuation),
            ("GetRequest", TypeName),
            (")", Punctuation),
            ("returns", Keyword),
            ("(", Punctuation),
            ("stream", Keyword),
            ("User", TypeName),
            (")", Punctuation),
            ("{", Punctuation)
        ]
    );
    // An option's brackets hold its name, not a type; a comment's opening in a string is text.
    assert!(texts_of(&lines[14..15], TypeName).is_empty());
    assert_eq!(class_of(&lines[14..15], "get"), Some(Key));
    assert_eq!(
        class_of(&lines[14..15], "\"/v1/{name=users/*}\""),
        Some(STRING)
    );
    assert_eq!(lines[15], vec![("}", Punctuation)]);
}

#[test]
fn proto_strings_take_both_quotes_and_end_with_their_line() {
    let lines = lexed(
        Lang::Proto,
        "option a = 'it\\'s';\noption b = \"open\nint32 c = 0x1F;",
    );
    assert_eq!(class_of(&lines[0..1], "'it\\'s'"), Some(STRING));
    assert_eq!(
        lines[1],
        vec![
            ("option", Keyword),
            ("b", Key),
            ("=", Operator),
            ("\"", Invalid),
            ("open", STRING)
        ]
    );
    assert_eq!(class_of(&lines[2..3], "0x1F"), Some(Number));
}

const GRAPHQL: &str = concat!(
    "\"\"\"\n",
    "A character.\n",
    "\"\"\"\n",
    "type Human implements Character & Node @key(fields: \"id\") {\n",
    "  id: ID!\n",
    "  friends(first: Int = 10): [Character]  # paged\n",
    "  type: String\n",
    "}\n",
    "union SearchResult = Human | Droid\n",
    "query Hero($episode: Episode = JEDI, $n: Float = -1.5e3) {\n",
    "  hero(episode: $episode) {\n",
    "    smallPic: profilePic(size: 64)\n",
    "    ...HeroFields\n",
    "    ... on Droid { primaryFunction }\n",
    "    friends @include(if: true) { name }\n",
    "  }\n",
    "}\n",
    "\"\"\"Say \\\"\"\" inside\"\"\" scalar DateTime",
);

#[test]
fn graphql_type_system_and_block_strings() {
    let lines = lexed(Lang::GraphQl, GRAPHQL);
    assert_eq!(lines[0], vec![("\"\"\"", STRING)]);
    assert_eq!(lines[1], vec![("A character.", STRING)]);
    assert_eq!(lines[2], vec![("\"\"\"", STRING)]);
    assert_eq!(
        lines[3],
        vec![
            ("type", Keyword),
            ("Human", TypeName),
            ("implements", Keyword),
            ("Character", TypeName),
            ("&", Operator),
            ("Node", TypeName),
            ("@key", Function),
            ("(", Punctuation),
            ("fields", Key),
            (":", Punctuation),
            ("\"id\"", STRING),
            (")", Punctuation),
            ("{", Punctuation)
        ]
    );
    assert_eq!(
        lines[4],
        vec![
            ("id", Key),
            (":", Punctuation),
            ("ID", TypeName),
            ("!", Operator)
        ]
    );
    assert_eq!(
        lines[5],
        vec![
            ("friends", Function),
            ("(", Punctuation),
            ("first", Key),
            (":", Punctuation),
            ("Int", TypeName),
            ("=", Operator),
            ("10", Number),
            (")", Punctuation),
            (":", Punctuation),
            ("[", Punctuation),
            ("Character", TypeName),
            ("]", Punctuation),
            ("# paged", Comment)
        ]
    );
    // A keyword before `:` is a field's name.
    assert_eq!(lines[6][0], ("type", Key));
    assert_eq!(
        lines[8],
        vec![
            ("union", Keyword),
            ("SearchResult", TypeName),
            ("=", Operator),
            ("Human", TypeName),
            ("|", Operator),
            ("Droid", TypeName)
        ]
    );
    assert_eq!(
        lines[17],
        vec![
            ("\"\"\"Say \\\"\"\" inside\"\"\"", STRING),
            ("scalar", Keyword),
            ("DateTime", TypeName)
        ]
    );
}

#[test]
fn graphql_operations_variables_directives_and_fragments() {
    let lines = lexed(Lang::GraphQl, GRAPHQL);
    assert_eq!(
        lines[9],
        vec![
            ("query", Keyword),
            ("Hero", Function),
            ("(", Punctuation),
            ("$episode", Key),
            (":", Punctuation),
            ("Episode", TypeName),
            ("=", Operator),
            (",", Punctuation),
            ("$n", Key),
            (":", Punctuation),
            ("Float", TypeName),
            ("=", Operator),
            ("-1.5e3", Number),
            (")", Punctuation),
            ("{", Punctuation)
        ]
    );
    // An enum value is no type.
    assert_eq!(class_of(&lines[9..10], "JEDI"), None);
    assert_eq!(class_of(&lines[10..11], "hero"), Some(Function));
    assert_eq!(class_of(&lines[10..11], "episode"), Some(Key));
    assert_eq!(
        lines[11],
        vec![
            ("smallPic", Key),
            (":", Punctuation),
            ("profilePic", Function),
            ("(", Punctuation),
            ("size", Key),
            (":", Punctuation),
            ("64", Number),
            (")", Punctuation)
        ]
    );
    assert_eq!(lines[12], vec![("...", Operator), ("HeroFields", Function)]);
    assert_eq!(
        lines[13],
        vec![
            ("...", Operator),
            ("on", Keyword),
            ("Droid", TypeName),
            ("{", Punctuation),
            ("}", Punctuation)
        ]
    );
    assert_eq!(
        lines[14],
        vec![
            ("@include", Function),
            ("(", Punctuation),
            ("if", Key),
            (":", Punctuation),
            ("true", Literal),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation)
        ]
    );
}

#[test]
fn graphql_strings_end_with_their_line_and_block_strings_go_on() {
    let lines = lexed(Lang::GraphQl, "name: \"open\nx: 1");
    assert_eq!(
        lines[0],
        vec![
            ("name", Key),
            (":", Punctuation),
            ("\"", Invalid),
            ("open", STRING)
        ]
    );
    assert_eq!(
        lines[1],
        vec![("x", Key), (":", Punctuation), ("1", Number)]
    );
    let mut tokens: Vec<Token> = Vec::new();
    let state = lex_line(
        Lang::GraphQl,
        b"x: \"\"\"open",
        LineState::default(),
        &mut tokens,
    );
    assert_eq!(state.open, Open::BlockString);
}

#[test]
fn the_same_text_is_read_by_each_language_s_rules() {
    // `#` is GraphQL's comment and nothing in Protobuf; `//` the other way round.
    let proto = lexed(Lang::Proto, "# x\n// y");
    let graphql = lexed(Lang::GraphQl, "# x\n// y");
    assert!(texts_of(&proto[0..1], Comment).is_empty());
    assert_eq!(proto[1], vec![("// y", Comment)]);
    assert_eq!(graphql[0], vec![("# x", Comment)]);
    assert!(texts_of(&graphql[1..2], Comment).is_empty());
}
