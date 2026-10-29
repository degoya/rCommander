use crate::lexer::js_words::generic_len;
use crate::test_support::{assert_plain, assert_tokens, highlighted, line};
use crate::types::TokenClass::{
    Function, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

#[test]
fn type_arguments_are_told_from_comparisons() {
    assert_eq!(generic_len(b"<T>(x)"), 3);
    assert_eq!(generic_len(b"<string, Array<number>> = x"), 23);
    assert_eq!(generic_len(b"<K extends keyof T>"), 19);
    assert_eq!(generic_len(b"<T = () => void>"), 16);
    assert_eq!(generic_len(b"< b;"), 0);
    assert_eq!(generic_len(b"< b) {"), 0);
    assert_eq!(generic_len(b"<b && c > d"), 0);
    assert_eq!(generic_len(b"< 10; i++) x > 1"), 0);
    assert_eq!(generic_len(b"<T"), 0);
}

#[test]
fn typescript_has_generics_and_annotations() {
    let text = concat!(
        "interface Point<T> { x: number; y?: T }\n",
        "function id<T>(value: T): T { return value; }\n",
        "const list: Array<string> = [];\n",
        "let less = a < b;\n",
        "type Alias = Foo;\n",
        "const n = x as Bar satisfies Baz;\n",
        "export abstract class Shape implements Drawable { private readonly size = 1; }\n",
    );
    let lines = highlighted("a.ts", text.as_bytes());
    assert_tokens(
        &lines[0],
        &[
            (Keyword, "interface"),
            (TypeName, "Point"),
            (Punctuation, "<"),
            (TypeName, "T"),
            (Punctuation, ">"),
            (Key, "x"),
            (TypeName, "number"),
            (Key, "y"),
        ],
    );
    assert_tokens(
        &lines[1],
        &[
            (Function, "id"),
            (TypeName, "T"),
            (Key, "value"),
            (Keyword, "return"),
        ],
    );
    assert_tokens(
        &lines[2],
        &[
            (Key, "list"),
            (TypeName, "Array"),
            (Punctuation, "<"),
            (TypeName, "string"),
            (Punctuation, ">"),
        ],
    );
    assert_tokens(&lines[3], &[(Operator, "<")]);
    assert_plain(&lines[3], "b");
    assert_tokens(&lines[4], &[(Keyword, "type"), (TypeName, "Alias")]);
    assert_tokens(
        &lines[5],
        &[
            (Keyword, "as"),
            (TypeName, "Bar"),
            (Keyword, "satisfies"),
            (TypeName, "Baz"),
        ],
    );
    assert_tokens(
        &lines[6],
        &[
            (Keyword, "export"),
            (Keyword, "abstract"),
            (TypeName, "Shape"),
            (Keyword, "implements"),
            (TypeName, "Drawable"),
            (Keyword, "private"),
            (Keyword, "readonly"),
        ],
    );
}

#[test]
fn a_typescript_word_is_a_name_in_javascript() {
    let tokens = line("a.js", "let type = interface;");
    assert_plain(&tokens, "type");
    assert_plain(&tokens, "interface");
}

#[test]
fn a_type_argument_list_closes_with_both_angles() {
    let tokens = line("a.ts", "let m: Map<string, Array<number>> = new Map();");
    let angles: Vec<_> = tokens
        .iter()
        .filter(|(_, text)| text == "<" || text == ">")
        .collect();
    assert_eq!(angles.len(), 4, "{tokens:?}");
    assert!(angles.iter().all(|(class, _)| *class == Punctuation));
    assert_tokens(&tokens, &[(TypeName, "Map"), (TypeName, "number")]);
}

#[test]
fn jsx_elements_attributes_and_children() {
    let text = concat!(
        "const App = () => <div className=\"x\" onClick={() => go(1)}>Hi {name} &amp; <Item.Row /></div>;\n",
        "const frag = <>\n",
        "  <b>{list.map(i => <i key={i}>{i}</i>)}</b>\n",
        "</>;\n",
    );
    let lines = highlighted("a.jsx", text.as_bytes());
    assert_tokens(
        &lines[0],
        &[
            (Operator, "=>"),
            (TypeName, "div"),
            (Key, "className"),
            (String, "\"x\""),
            (Key, "onClick"),
            (Punctuation, "{"),
            (Function, "go"),
            (Number, "1"),
            (Literal, "&amp;"),
            (TypeName, "Item.Row"),
            (Punctuation, "/>"),
            (Punctuation, "</"),
            (Punctuation, ";"),
        ],
    );
    assert_plain(&lines[0], "Hi ");
    assert_tokens(&lines[1], &[(Punctuation, "<"), (Punctuation, ">")]);
    assert_tokens(
        &lines[2],
        &[
            (TypeName, "b"),
            (Function, "map"),
            (TypeName, "i"),
            (Key, "key"),
        ],
    );
    assert_eq!(
        lines[3],
        vec![
            (Punctuation, "</".to_owned()),
            (Punctuation, ">".to_owned()),
            (Punctuation, ";".to_owned()),
        ]
    );
}

#[test]
fn tsx_has_both_types_and_elements() {
    let text = "function f<T>(x: T) { return <p title='a'>{x as string}</p>; }\n";
    let tokens = line("a.tsx", text);
    assert_tokens(
        &tokens,
        &[
            (Function, "f"),
            (TypeName, "T"),
            (TypeName, "p"),
            (Key, "title"),
            (String, "'a'"),
            (Keyword, "as"),
            (TypeName, "string"),
        ],
    );
}

#[test]
fn a_comparison_in_jsx_code_is_no_element() {
    let tokens = line("a.jsx", "if (a < b) { x = 1 }");
    assert_tokens(&tokens, &[(Operator, "<")]);
}
