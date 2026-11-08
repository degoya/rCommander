use std::cell::Cell;

use rc_engine::{PanelId, VfsPath};

use iced::widget::markdown::Item;

use super::{Look, Selection, code_font, code_style, link_tip, paint, select, settings, view};
use crate::{
    app::viewer::{lister::Lister, markdown},
    i18n::t,
    theme,
};

#[test]
fn the_tooltip_names_each_target_and_marks_one_that_never_opens() {
    let tip = link_tip(&[
        "https://example.com".to_owned(),
        "javascript:alert(1)".to_owned(),
    ]);
    let lines: Vec<&str> = tip.lines().collect();
    assert_eq!(lines[0], "https://example.com");
    assert_eq!(
        lines[1],
        t!("viewer.link_never_opens", url = "javascript:alert(1)")
    );
}

#[test]
fn code_is_in_the_viewers_font_and_links_in_the_accent_in_both_looks() {
    let viewer = theme::fonts::fonts().viewer;
    for dark in [false, true] {
        let tokens = theme::tokens(dark);
        let shown = settings(tokens, viewer);
        assert_eq!(shown.style.code_block_font, viewer.font);
        assert_eq!(shown.style.inline_code_font, viewer.font);
        assert_eq!(shown.style.font, theme::UI_FONT);
        assert_eq!(shown.style.link_color, tokens.accent);
        assert_eq!(shown.style.inline_code_color, tokens.text);
        assert_eq!(shown.code_size.0, viewer.size);
    }
    // A window's own text size sizes prose and code alike (RC-020-12).
    let larger = theme::fonts::AreaFont {
        size: 20.0,
        ..viewer
    };
    let shown = settings(theme::tokens(false), larger);
    assert_eq!(shown.text_size.0, 20.0);
    assert_eq!(shown.code_size.0, 20.0);
}

#[test]
fn the_hostile_fixture_draws() {
    let source = include_str!("../../../tests/fixtures/markdown/hostile.md");
    let preview =
        markdown::parse(source.as_bytes(), rc_engine::viewer::detect(b"")).expect("parses");
    let lister = Lister::new(7, PanelId::Left, VfsPath::local("/x/hostile.md"));
    for dark in [false, true] {
        let _element = view(&lister, &preview.items, theme::tokens(dark));
    }
}

#[test]
fn the_preview_numbers_its_blocks_as_the_parse_collects_them() {
    let source = include_str!("../../../tests/fixtures/markdown/hostile.md");
    let preview =
        markdown::parse(source.as_bytes(), rc_engine::viewer::detect(b"")).expect("parses");
    let collected = select::blocks(&preview.items);
    let selection = Selection::default();
    let look = Look {
        tokens: theme::tokens(false),
        selection: Some(&selection),
        next: Cell::new(0),
        code: None,
        next_code: Cell::new(0),
    };
    let settings = settings(theme::tokens(false), theme::fonts::fonts().viewer);
    let _element = iced::widget::markdown::view_with(&preview.items, settings, &look);
    assert!(!collected.is_empty());
    assert_eq!(look.next.get(), collected.len());
}

/// The first code block of `source`'s lines.
fn code_lines(source: &str) -> Vec<iced::widget::markdown::Text> {
    let preview =
        markdown::parse(source.as_bytes(), rc_engine::viewer::detect(b"")).expect("parses");
    preview
        .items
        .into_iter()
        .find_map(|item| match item {
            Item::CodeBlock { lines, .. } => Some(lines),
            _ => None,
        })
        .expect("a code block")
}

#[test]
fn code_is_monospaced_even_when_the_viewers_font_is_not() {
    let proportional = theme::fonts::AreaFont {
        font: theme::UI_FONT,
        size: 13.0,
        mono: false,
    };
    assert_eq!(code_font(proportional), theme::MONO_FONT);
    let shown = settings(theme::tokens(false), proportional);
    assert_eq!(shown.style.code_block_font, theme::MONO_FONT);
    assert_eq!(shown.style.inline_code_font, theme::MONO_FONT);
    // A monospaced choice of the user stays theirs.
    let chosen = theme::fonts::AreaFont {
        font: iced::Font::with_name("Fira Code"),
        size: 13.0,
        mono: true,
    };
    assert_eq!(code_font(chosen), chosen.font);
}

#[test]
fn a_code_blocks_lines_are_drawn_in_the_code_font_not_the_prose_font() {
    let shown = settings(theme::tokens(false), theme::fonts::fonts().viewer);
    let lines = code_lines("Text\n\n```rust\nlet x = 1;\n```\n");
    let spans = lines[0].spans(code_style(shown.style));
    assert!(!spans.is_empty());
    for span in spans.iter() {
        assert_eq!(
            span.font,
            Some(shown.style.code_block_font),
            "{:?}",
            span.text
        );
    }
    // What iced's own style gives each span: the prose font, which the code font must replace.
    let prose = lines[0].spans(shown.style);
    assert_eq!(prose[0].font, Some(theme::UI_FONT));
}

#[test]
fn a_coloured_code_line_is_cut_at_its_tokens_in_the_themes_colours() {
    use crate::app::viewer::{code_lexer, document::TokenClass, spans::color};
    let shown = settings(theme::tokens(true), theme::fonts::fonts().viewer);
    let lines = code_lines("```rust\nlet x = 1;\n```\n");
    let plain = lines[0].spans(code_style(shown.style));
    let lang = code_lexer::lang_of("rust").expect("rust");
    let tokens = code_lexer::lex_line(lang, "let x = 1;", &mut None);
    let syntax = &theme::tokens(true).syntax;
    let painted = paint(plain.clone(), Some(&tokens), syntax);
    let joined: String = painted.iter().map(|span| span.text.as_ref()).collect();
    assert_eq!(joined, "let x = 1;", "the text is the same, only cut");
    let keyword = painted
        .iter()
        .find(|span| span.text.as_ref() == "let")
        .expect("cut");
    assert_eq!(keyword.color, Some(color(syntax, TokenClass::Keyword)));
    assert_eq!(keyword.font, Some(shown.style.code_block_font));
    // Without tokens, or with tokens that do not fit, the line is drawn as it was.
    assert_eq!(paint(plain.clone(), None, syntax).len(), plain.len());
    let wrong = vec![(0..99, TokenClass::Keyword)];
    assert_eq!(
        paint(plain.clone(), Some(&wrong), syntax).len(),
        plain.len()
    );
}

#[test]
fn a_highlighted_preview_draws_with_its_code_tokens_and_selects_as_before() {
    let source = "# T\n\n```sh\necho $HOME\n```\n\n```\nplain\n```\n";
    let mut preview =
        markdown::parse(source.as_bytes(), rc_engine::viewer::detect(b"")).expect("parses");
    preview.code = Some(crate::app::viewer::code_lexer::colours(&preview.items));
    let collected = select::blocks(&preview.items);
    let selection = Selection::default();
    let code = preview.code.as_deref();
    let look = Look {
        tokens: theme::tokens(false),
        selection: Some(&selection),
        next: Cell::new(0),
        code,
        next_code: Cell::new(0),
    };
    let settings = settings(theme::tokens(false), theme::fonts::fonts().viewer);
    let _element = iced::widget::markdown::view_with(&preview.items, settings, &look);
    assert_eq!(
        look.next.get(),
        collected.len(),
        "colours change no block's number"
    );
    assert_eq!(look.next_code.get(), 2);
}
