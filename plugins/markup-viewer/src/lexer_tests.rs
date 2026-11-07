use super::*;

/// The state after `lines`, each lexed with the next beside it.
fn state_after(lang: Lang, lines: &[&str]) -> LineState {
    let mut state = LineState::default();
    for (index, line) in lines.iter().enumerate() {
        let next = lines.get(index + 1).map(|next| next.as_bytes());
        state = lex_line(lang, line.as_bytes(), next, state, &mut ());
    }
    state
}

#[test]
fn a_tag_holds_at_most_max_tag_bytes() {
    assert_eq!(
        Tag::new(b"verbatim").map(|tag| tag.bytes().to_vec()),
        Some(b"verbatim".to_vec())
    );
    assert!(Tag::new(b"").is_none());
    assert!(Tag::new(&[b'a'; MAX_TAG + 1]).is_none());
}

#[test]
fn what_reaches_into_the_next_line_is_kept_in_the_state() {
    let title = lex_line(
        Lang::Rst,
        b"Title",
        Some(b"=====".as_slice()),
        LineState::default(),
        &mut (),
    );
    assert!(title.underline);
    assert_eq!(
        state_after(Lang::Rst, &["Title", "=====", ""]),
        LineState::default()
    );
    assert_eq!(
        state_after(Lang::Rst, &["Example::"]).block,
        Block::Expect { indent: 0 }
    );
    assert_eq!(
        state_after(Lang::Rst, &["  .. code:: rust", "", "     let x = 1;"]).block,
        Block::Indented {
            indent: 2,
            body: Body::Code
        }
    );
    assert_eq!(
        state_after(Lang::Latex, &["\\begin{lstlisting}"]).block,
        Block::Verbatim {
            tag: Tag::new(b"lstlisting").expect("a short name"),
            comment: false
        }
    );
    assert_eq!(
        state_after(Lang::Latex, &["$a"]).block,
        Block::Math(MathClose::Dollar)
    );
    assert_eq!(state_after(Lang::Latex, &["$a", ""]), LineState::default());
    assert_eq!(
        state_after(Lang::Bibtex, &["@a{k, t = {x"]).block,
        Block::Entry(Entry {
            close: b'}',
            key: false,
            value: Value::Braced(0)
        })
    );
}

#[test]
fn openings_left_open_are_reported_in_the_order_of_the_file() {
    let mut opens = Opens::default();
    let lines = ["$x", "", "\\[ y \\]", "\\begin{comment}", "z"];
    let mut state = LineState::default();
    for (index, line) in lines.iter().enumerate() {
        let next = lines.get(index + 1).map(|next| next.as_bytes());
        state = lex_line(Lang::Latex, line.as_bytes(), next, state, &mut opens);
        opens.line += 1;
    }
    assert_eq!(
        opens.into_unclosed(),
        vec![
            QuoteOpen {
                line: 0,
                col: 0,
                width: 1
            },
            QuoteOpen {
                line: 3,
                col: 0,
                width: 6
            },
        ]
    );
}

#[test]
fn a_line_gets_at_most_max_tokens_per_line() {
    let latex = "\\a ".repeat(MAX_TOKENS_PER_LINE + 10);
    let bibtex = format!("@a{{k, {}}}", "x = 1, ".repeat(MAX_TOKENS_PER_LINE));
    let rst = "*a* ".repeat(MAX_TOKENS_PER_LINE + 10);
    for (lang, line) in [
        (Lang::Latex, latex),
        (Lang::Bibtex, bibtex),
        (Lang::Rst, rst),
    ] {
        let mut tokens: Vec<Token> = Vec::new();
        lex_line(
            lang,
            line.as_bytes(),
            None,
            LineState::default(),
            &mut tokens,
        );
        assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE, "{lang:?}");
    }
}
