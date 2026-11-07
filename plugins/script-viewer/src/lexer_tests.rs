use super::*;

fn state_after(lang: Lang, lines: &[&str]) -> LineState {
    lines.iter().fold(LineState::default(), |state, line| {
        lex_line(lang, line.as_bytes(), state, &mut ())
    })
}

#[test]
fn a_tag_holds_at_most_max_tag_bytes() {
    assert_eq!(
        Tag::new(b"EOS").map(|tag| tag.bytes().to_vec()),
        Some(b"EOS".to_vec())
    );
    assert!(Tag::new(b"").is_none());
    assert!(Tag::new(&[b'A'; MAX_TAG]).is_some());
    assert!(Tag::new(&[b'A'; MAX_TAG + 1]).is_none());
}

#[test]
fn a_heredoc_ends_at_its_tag_indented_only_when_it_may_be() {
    let tag = Tag::new(b"EOS").expect("a short tag");
    let plain = Heredoc {
        tag,
        indented: false,
        interp: true,
    };
    let indented = Heredoc {
        indented: true,
        ..plain
    };
    assert!(ends_heredoc(b"EOS", &plain));
    assert!(!ends_heredoc(b"  EOS", &plain));
    assert!(!ends_heredoc(b"EOS;", &plain));
    assert!(ends_heredoc(b"\t  EOS", &indented));
    assert!(!ends_heredoc(b"EOSX", &indented));
}

#[test]
fn what_reaches_into_the_next_line_is_kept_in_the_state() {
    let open_string = state_after(Lang::Ruby, &["s = \"open"]);
    assert!(matches!(open_string.block, Block::Str(_)));
    assert_eq!(
        state_after(Lang::Ruby, &["s = \"open", "closed\""]),
        LineState::default()
    );
    assert!(matches!(
        state_after(Lang::Ruby, &["x = <<~EOS"]).block,
        Block::Heredoc(_)
    ));
    assert_eq!(
        state_after(Lang::Ruby, &["=begin"]).block,
        Block::Doc { data: false }
    );
    assert_eq!(state_after(Lang::Perl, &["__END__"]).block, Block::Data);
    assert_eq!(
        state_after(Lang::Perl, &["__DATA__", "=head1 X"]).block,
        Block::Doc { data: true }
    );
    assert_eq!(
        state_after(Lang::Perl, &["__DATA__", "=head1 X", "=cut"]).block,
        Block::Data
    );
    assert_eq!(
        state_after(Lang::Lua, &["--[=[ open"]).block,
        Block::Long {
            level: 1,
            comment: true
        }
    );
    // A one-line string ends with its line in Lua and VBScript.
    assert_eq!(state_after(Lang::Lua, &["s = 'open"]), LineState::default());
    assert_eq!(
        state_after(Lang::VbScript, &["s = \"open"]),
        LineState::default()
    );
}

#[test]
fn open_and_failed_strings_are_reported_in_the_order_of_the_file() {
    let mut opens = Opens::default();
    for line in ["x = 'one'", "y = \"open", "z = 1"] {
        lex_line(Lang::Lua, line.as_bytes(), LineState::default(), &mut opens);
        opens.line += 1;
    }
    let mut state = LineState::default();
    for line in ["s = <<~EOS", "body"] {
        state = lex_line(Lang::Ruby, line.as_bytes(), state, &mut opens);
        opens.line += 1;
    }
    assert!(matches!(state.block, Block::Heredoc(_)));
    assert_eq!(
        opens.into_unclosed(),
        vec![
            QuoteOpen {
                line: 1,
                col: 4,
                width: 1
            },
            QuoteOpen {
                line: 3,
                col: 4,
                width: 6
            },
        ]
    );
}

#[test]
fn a_line_gets_at_most_max_tokens_per_line() {
    let line = "1, ".repeat(MAX_TOKENS_PER_LINE);
    for lang in [Lang::Ruby, Lang::Perl, Lang::Lua, Lang::VbScript] {
        let mut tokens: Vec<Token> = Vec::new();
        lex_line(lang, line.as_bytes(), LineState::default(), &mut tokens);
        assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE, "{lang:?}");
    }
}
