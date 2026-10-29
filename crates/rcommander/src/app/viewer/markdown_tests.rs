//! The Markdown preview without a window: what is found, what is parsed, what never opens, and
//! the lister's states behind `5`.

use iced::widget::markdown::{Bullet, Item, Style};
use pulldown_cmark::{Event, Parser};
use rc_db::Db;
use rc_engine::{Command, Engine, PanelId, VfsPath, viewer::ViewMode};
use tempfile::TempDir;

use super::{
    ALL_CODES, Markdown, PREVIEW_LIMIT, Preview, PreviewMessage, html_as_text, is_markdown,
    options, parse, render,
};
use crate::app::harness::headless_engine;
use crate::{
    app::{
        harness::Harness,
        viewer::{
            ViewerMessage,
            keys::{ListerKey, Scroll},
            links,
            lister::{Lister, Request},
            load,
        },
    },
    i18n::{self, t},
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/markdown/hostile.md");

fn style() -> Style {
    Style::from_palette(iced::Theme::Light.palette())
}

/// Every piece of text the items show, and every link target, in order.
fn walk(items: &[Item], text: &mut String, targets: &mut Vec<String>) {
    let style = style();
    for item in items {
        match item {
            Item::Heading(_, line) | Item::Paragraph(line) => {
                for span in line.spans(style).iter() {
                    text.push_str(&span.text);
                    targets.extend(span.link.clone());
                }
                text.push('\n');
            }
            Item::Image { alt, .. } => {
                for span in alt.spans(style).iter() {
                    text.push_str(&span.text);
                }
            }
            Item::CodeBlock { code, .. } => text.push_str(code),
            Item::List { bullets, .. } => {
                for bullet in bullets {
                    let (Bullet::Point { items } | Bullet::Task { items, .. }) = bullet;
                    walk(items, text, targets);
                }
            }
            Item::Quote(items) => walk(items, text, targets),
            Item::Table { columns, .. } => {
                for column in columns {
                    walk(&column.header, text, targets);
                }
            }
            Item::Rule => {}
        }
    }
}

fn fixture() -> Preview {
    parse(
        FIXTURE.as_bytes(),
        rc_engine::viewer::detect(FIXTURE.as_bytes()),
    )
    .expect("parses")
}

#[test]
fn md_and_markdown_files_are_previewed_and_no_other() {
    for name in ["README.md", "notes.MD", "guide.markdown", "a.b.Markdown"] {
        assert!(is_markdown(&VfsPath::local(name)), "{name}");
    }
    for name in ["notes.txt", "page.mdx", "md", "index.html", "README"] {
        assert!(!is_markdown(&VfsPath::local(name)), "{name}");
    }
}

#[test]
fn raw_html_becomes_text_and_code_keeps_its_tags() {
    let text = html_as_text(FIXTURE).expect("settles");
    let html = Parser::new_ext(&text, options())
        .filter(|event| matches!(event, Event::Html(_) | Event::InlineHtml(_)))
        .count();
    assert_eq!(html, 0, "no HTML is left for the parser");
    assert!(text.contains("`<kbd>`"), "a code span keeps its `<`");
    assert!(
        text.contains("<p>code keeps its tags</p>"),
        "a code block too"
    );

    let mut shown = String::new();
    let mut targets = Vec::new();
    walk(&fixture().items, &mut shown, &mut targets);
    for raw in [
        "<script>alert(1)</script>",
        "<div onclick=\"steal()\">",
        "<img src=\"https://tracker.example.com/html.png\">",
        "<b>bold</b>",
        "<!-- a comment -->",
    ] {
        assert!(shown.contains(raw), "{raw} is shown as text in {shown:?}");
    }
    assert!(!html_as_text("plain *text*").is_some_and(|t| t.contains("&lt;")));
}

#[test]
fn a_remote_image_is_its_alt_text_and_nothing_is_fetched() {
    let items = fixture().items;
    let images: Vec<(String, String)> = items
        .iter()
        .filter_map(|item| match item {
            Item::Image { url, alt, .. } => Some((
                url.clone(),
                alt.spans(style())
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        images,
        vec![(
            "https://tracker.example.com/pixel.png".to_owned(),
            "remote alt".to_owned()
        )],
        "the Markdown image only; the <img> of the HTML is text"
    );
}

#[test]
fn only_http_https_and_mailto_links_of_the_file_open() {
    let mut shown = String::new();
    let mut targets = Vec::new();
    walk(&fixture().items, &mut shown, &mut targets);
    for target in [
        "https://example.com/page",
        "mailto:someone@example.com",
        "javascript:alert(1)",
        "file:///C:/Windows/System32/calc.exe",
        "other.md",
        "https://example.org/auto",
    ] {
        assert!(targets.iter().any(|t| t == target), "{target} is a link");
    }
    let opening: Vec<&String> = targets.iter().filter(|t| links::opens(t)).collect();
    assert_eq!(
        opening,
        vec![
            "https://example.com/page",
            "mailto:someone@example.com",
            "https://example.org/auto"
        ]
    );
}

#[test]
fn headings_lists_quotes_code_and_tables_are_items() {
    let items = fixture().items;
    assert!(matches!(items.first(), Some(Item::Heading(..))));
    assert!(items.iter().any(|i| matches!(i, Item::List { .. })));
    assert!(items.iter().any(|i| matches!(i, Item::Quote(_))));
    assert!(items.iter().any(|i| matches!(i, Item::CodeBlock { .. })));
    assert!(items.iter().any(|i| matches!(i, Item::Table { .. })));
}

#[test]
fn every_preview_error_has_a_message() {
    for code in ALL_CODES {
        assert_ne!(i18n::error_text(code), *code, "error.{code} is missing");
    }
}

/// A lister on `name` holding `bytes`, opened; the requests of the opening.
async fn lister_on(name: &str, bytes: &[u8]) -> (TempDir, Engine, Lister, Vec<Request>) {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join(name));
    std::fs::write(&path.path, bytes).expect("write");
    let engine = headless_engine(root.path()).await;
    let opened = load::open(engine.clone(), path.clone(), None)
        .await
        .expect("opens");
    let mut lister = Lister::new(1, PanelId::Left, path);
    let requests = lister.opened(opened);
    (root, engine, lister, requests)
}

fn asks_render(requests: &[Request]) -> Option<bool> {
    requests.iter().find_map(|request| match request {
        Request::Render { forced } => Some(*forced),
        _ => None,
    })
}

#[tokio::test]
async fn a_markdown_file_asks_for_its_preview_and_5_turns_it_to_text_and_back() {
    let (_root, _engine, mut lister, requests) = lister_on("a.md", b"# Title\n\ntext\n").await;
    assert_eq!(asks_render(&requests), Some(false));
    assert!(lister.preview_waiting(), "the view waits, no text flashes");
    let file = lister.file.as_ref().expect("open").file.clone();
    let rendered = render(file, lister.detected, None, false).await;
    let path = lister.path.clone();
    lister.rendered(&path, rendered);
    assert!(lister.preview().is_some(), "shown");

    assert!(lister.key(ListerKey::Preview).is_empty());
    assert!(lister.preview().is_none(), "5: the text");
    assert!(lister.key(ListerKey::Preview).is_empty());
    assert!(lister.preview().is_some(), "5 again: the preview");

    assert!(matches!(
        lister.key(ListerKey::Scroll(Scroll::PageDown)).as_slice(),
        [Request::PreviewScroll(Scroll::PageDown)]
    ));
    lister.key(ListerKey::Mode(ViewMode::Hex));
    assert!(lister.preview().is_none(), "3: hex, the preview hidden");
    lister.key(ListerKey::Preview);
    assert_eq!(lister.mode, ViewMode::Text, "5 from hex: the preview");
    assert!(lister.preview().is_some());
}

#[tokio::test]
async fn another_file_asks_nothing_and_5_does_nothing() {
    let (_root, _engine, mut lister, requests) = lister_on("a.txt", b"# Title\n").await;
    assert_eq!(asks_render(&requests), None);
    assert!(lister.markdown.is_none());
    assert!(lister.key(ListerKey::Preview).is_empty());
    assert!(!lister.preview_waiting());
}

#[tokio::test]
async fn with_the_setting_off_the_file_is_text_until_5() {
    let (_root, _engine, mut lister, _) = lister_on("a.md", b"# Title\n").await;
    let db = Db::open_in_memory().await.expect("db");
    db.set(rc_engine::settings::MARKDOWN_PREVIEW_KEY, &false)
        .await
        .expect("set");
    let file = lister.file.as_ref().expect("open").file.clone();
    let rendered = render(file.clone(), lister.detected, Some(db.clone()), false).await;
    assert!(matches!(rendered, Ok(None)), "off: no preview");
    let path = lister.path.clone();
    lister.rendered(&path, rendered);
    assert!(matches!(lister.markdown, Some(Markdown::Text)));
    assert!(lister.preview().is_none() && !lister.preview_waiting());

    assert_eq!(asks_render(&lister.key(ListerKey::Preview)), Some(true));
    let forced = render(file, lister.detected, Some(db), true).await;
    lister.rendered(&path, forced);
    assert!(lister.preview().is_some(), "5 shows it all the same");
}

#[tokio::test]
async fn a_file_over_the_limit_is_text_with_a_note() {
    let size = usize::try_from(PREVIEW_LIMIT).expect("fits") + 1;
    let (_root, _engine, mut lister, _) = lister_on("big.md", &vec![b'a'; size]).await;
    let file = lister.file.as_ref().expect("open").file.clone();
    let rendered = render(file, lister.detected, None, false).await;
    assert!(matches!(rendered, Err("markdown_too_large")));
    let path = lister.path.clone();
    lister.rendered(&path, rendered);
    assert!(lister.preview().is_none() && !lister.preview_waiting());
    assert_eq!(
        lister.note.as_deref(),
        Some(i18n::error_text("markdown_too_large").as_str())
    );
}

#[tokio::test]
async fn an_answer_for_another_file_is_dropped() {
    let (_root, _engine, mut lister, _) = lister_on("a.md", b"# Title\n").await;
    let other = VfsPath::local("/elsewhere/b.md");
    lister.rendered(&other, Ok(Some(fixture())));
    assert!(lister.preview_waiting(), "still waiting for its own");
}

#[tokio::test]
async fn in_the_window_5_switches_and_a_click_opens_only_an_allowed_link() {
    let root = tempfile::tempdir().expect("tempdir");
    std::fs::write(root.path().join("a.md"), b"[web](https://example.com)\n").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    let view = Command::ViewFile {
        panel: None,
        path: Some("a.md".to_owned()),
    };
    harness
        .app
        .engine()
        .dispatch(view)
        .await
        .expect("announced");
    harness.settle().await;
    let lister = harness.app.viewers.open.last().expect("a lister");
    let (id, path) = (lister.id, lister.path.clone());
    // The harness runs no tasks: the answers of the reads are handed over here.
    let opened = load::open(harness.app.engine().clone(), path.clone(), None)
        .await
        .expect("opens");
    let asked = harness.app.on_viewer(ViewerMessage::Opened(id, Ok(opened)));
    assert_eq!(asked.len(), 2, "the first page and the preview");
    let preview = parse(
        b"[web](https://example.com)\n",
        rc_engine::viewer::detect(b""),
    );
    let rendered = PreviewMessage::Rendered(path, preview.map(Some));
    harness.app.on_viewer(ViewerMessage::Preview(id, rendered));
    let shown = |harness: &Harness| harness.app.viewers.open[0].preview().is_some();
    assert!(shown(&harness));
    harness.press("5").await;
    assert!(!shown(&harness), "5: the text");
    harness.press("5").await;
    assert!(shown(&harness), "5: the preview again");

    let refused = PreviewMessage::Link("javascript:alert(1)".to_owned());
    assert!(
        harness.app.on_preview(id, refused).is_empty(),
        "nothing runs"
    );
    assert_eq!(
        harness.app.viewers.open[0].note,
        Some(t!("viewer.link_refused", url = "javascript:alert(1)"))
    );
    let file = PreviewMessage::Link("file:///C:/Windows/System32/calc.exe".to_owned());
    assert!(harness.app.on_preview(id, file).is_empty());
    let web = PreviewMessage::Link("https://example.com".to_owned());
    assert_eq!(
        harness.app.on_preview(id, web).len(),
        1,
        "the system gets it"
    );
}
