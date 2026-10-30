use std::cell::Cell;

use rc_engine::{PanelId, VfsPath};

use super::{Look, Selection, link_tip, select, settings, view};
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
    };
    let settings = settings(theme::tokens(false), theme::fonts::fonts().viewer);
    let _element = iced::widget::markdown::view_with(&preview.items, settings, &look);
    assert!(!collected.is_empty());
    assert_eq!(look.next.get(), collected.len());
}
