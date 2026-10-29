//! *Hostile output* (RC-100-08): a viewer that answers what it must not is refused or cut, and
//! the window never sees it as it was sent.

use rc_plugin_api::viewer::TokenClass;

use super::tests::{code, open, plugin, plugin_with};
use super::{MAX_TREE_CHILDREN, ViewerOptions};
use crate::{
    ExecutionOutcome, MAX_LOG_LINES_PER_CALL, MemorySink,
    failure::codes,
    test_fixtures::viewer_manifest,
    testbed::{self, DATA, Export, Testbed},
};

/// A test bed whose `open` offers `outputs` (names of the contract), with `title` and `pages`.
fn offering(outputs: &[&str], title: Option<&str>, pages: Option<u32>) -> Testbed {
    let mut bed = Testbed::new();
    let indices: Vec<u8> = outputs.iter().map(|name| testbed::output(name)).collect();
    let outputs = bed.data(&indices);
    let title = title.map(|title| bed.data(title.as_bytes()));
    bed.body(Export::Open, testbed::ok_document(outputs, title, pages))
}

fn bad_output_what<T>(result: Result<T, rc_plugin_types::Failure>) -> String {
    let failure = result.err().expect("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_BAD_OUTPUT));
    failure.params.get("what").cloned().unwrap_or_default()
}

#[tokio::test]
async fn a_picture_of_a_hundred_thousand_squared_pixels_is_refused() {
    let mut bed = offering(&["image"], None, None);
    let format = bed.data(b"rgba:100000x100000");
    let bed = bed.body(Export::Image, testbed::ok_pair(format));
    let plugin = plugin(&viewer_manifest(&["image"]), &bed);
    let mut session = open(&plugin).await.expect("open");
    assert_eq!(
        bad_output_what(session.image(Box::new(MemorySink::new())).await),
        "image_size"
    );
    let records = plugin.execution_log().records(&plugin.identity().id);
    assert_eq!(records[1].outcome, ExecutionOutcome::Refused);
    assert!(
        !session.is_closed(),
        "a refused answer leaves the instance usable"
    );
}

#[tokio::test]
async fn a_raster_of_the_wrong_length_is_refused() {
    let mut bed = offering(&["image"], None, None);
    let format = bed.data(b"rgba:2x2");
    let three = bed.data(&[1, 2, 3]);
    let image = format!(
        "{} {}",
        testbed::write(0, 0, three),
        testbed::ok_pair(format)
    );
    let plugin = plugin(
        &viewer_manifest(&["image"]),
        &bed.body(Export::Image, image),
    );
    let mut session = open(&plugin).await.expect("open");
    assert_eq!(
        bad_output_what(session.image(Box::new(MemorySink::new())).await),
        "raster_length"
    );
}

#[tokio::test]
async fn a_page_raster_of_the_wrong_length_is_refused() {
    let mut bed = offering(&["pages"], None, Some(3));
    let three = bed.data(&[1, 2, 3]);
    let page = format!("{} {}", testbed::write(3, 0, three), testbed::ok_unit());
    let plugin = plugin(&viewer_manifest(&["pages"]), &bed.body(Export::Page, page));
    let mut session = open(&plugin).await.expect("open");
    assert_eq!(session.document().pages, Some(3));
    assert_eq!(
        bad_output_what(session.page(0, 1, 1, Box::new(MemorySink::new())).await),
        "raster_length"
    );
    assert_eq!(
        code(session.page(3, 1, 1, Box::new(MemorySink::new())).await),
        codes::PLUGIN_REQUEST_TOO_LARGE,
        "a page past the count is not asked for"
    );
}

#[tokio::test]
async fn text_over_the_limit_is_refused_even_if_the_plugin_ignores_the_error() {
    let bed = Testbed::new();
    let text = format!(
        "{} {}",
        testbed::write(0, 900, (DATA, 200)),
        "(i32.store8 (i32.const 4096) (i32.const 0)) (i32.store (i32.const 4100) (i32.const 0)) \
         (i32.store (i32.const 4104) (i32.const 0)) (i32.const 4096)"
    );
    let options = ViewerOptions {
        output_limit: 1000,
        ..ViewerOptions::default()
    };
    let plugin = plugin_with(
        &viewer_manifest(&["text"]),
        &bed.body(Export::Text, text),
        options,
    );
    let mut session = open(&plugin).await.expect("open");
    let sink = MemorySink::new();
    assert_eq!(
        code(session.text(Box::new(sink.clone())).await),
        codes::PLUGIN_OUTPUT_TOO_LARGE
    );
    assert!(
        sink.contents().is_empty(),
        "nothing past the limit was written"
    );
}

#[tokio::test]
async fn control_and_direction_characters_in_a_title_are_spelled_out() {
    let bed = offering(&["text"], Some("Evil\u{202e}txt.exe\u{7}"), None);
    let plugin = plugin(&viewer_manifest(&["text"]), &bed);
    let session = open(&plugin).await.expect("open");
    assert_eq!(
        session.document().title.as_deref(),
        Some("Evil\\u{202e}txt.exe\\u{7}")
    );
}

#[tokio::test]
async fn a_tree_of_a_million_nodes_is_cut() {
    let bed =
        offering(&["tree"], None, None).body(Export::Children, testbed::many_nodes(1_000_000));
    let plugin = plugin(&viewer_manifest(&["tree"]), &bed);
    let mut session = open(&plugin).await.expect("open");
    let roots = session.children(None).await.expect("children");
    assert_eq!(roots.nodes.len(), MAX_TREE_CHILDREN);
    assert!(roots.truncated);
    assert_eq!(
        code(session.children(Some(u64::MAX)).await),
        codes::PLUGIN_UNKNOWN_NODE,
        "a node never handed out is not expanded"
    );
}

#[tokio::test]
async fn a_log_flood_is_cut_to_the_budget() {
    let mut bed = offering(&["text"], None, None);
    let level = bed.data(b"info");
    let message = bed.data(b"flood\nfake line");
    let outputs = bed.data(&[testbed::output("text")]);
    let open_body = format!(
        "{}{}",
        testbed::log_flood(10_000, level, message),
        testbed::ok_document(outputs, None, None)
    );
    let plugin = plugin(
        &viewer_manifest(&["text"]),
        &bed.body(Export::Open, open_body),
    );
    let session = open(&plugin).await.expect("open");
    let budget = &session.store.data().log_budget;
    assert_eq!(budget.written(), MAX_LOG_LINES_PER_CALL);
    assert_eq!(
        budget.suppressed(),
        10_000 - u64::from(MAX_LOG_LINES_PER_CALL)
    );
}

#[tokio::test]
async fn overlapping_or_foreign_tokens_leave_their_line_uncoloured() {
    let mut bed = offering(&["highlighted", "text"], None, None);
    let overlapping = bed.data(&testbed::token_bytes(&[(0, 3, 6), (2, 3, 2)]));
    let past_the_line = bed.data(&testbed::token_bytes(&[(0, 50, 5)]));
    let fine = bed.data(&testbed::token_bytes(&[(0, 1, 6), (1, 3, 2)]));
    // `data` answers a length in bytes; a `list<token>` counts tokens of 12 bytes.
    let tokens = |(at, bytes): (u32, u32)| (at, bytes / 12);
    let lists = bed.data(&testbed::list_bytes(&[
        tokens(overlapping),
        tokens(past_the_line),
        tokens(fine),
    ]));
    let bed = bed.body(Export::Tokens, testbed::ok_pair((lists.0, 3)));
    let plugin = plugin(&viewer_manifest(&["highlighted", "text"]), &bed);
    let mut session = open(&plugin).await.expect("open");
    let lines = ["{\"a\": 1}", "short", "{\"b\"", "no answer"];
    let shown = session.tokens(10, &lines).await.expect("tokens");
    assert_eq!(shown.len(), lines.len());
    assert!(shown[0].is_none(), "overlapping spans");
    assert!(shown[1].is_none(), "a span past its line");
    let spans = shown[2].as_ref().expect("valid spans");
    assert_eq!(spans[1].class, TokenClass::String);
    assert!(shown[3].is_none(), "no list for the line");
}

#[tokio::test]
async fn outputs_twice_empty_or_not_in_the_manifest_are_refused() {
    for (outputs, what) in [
        (&["text", "text"][..], "outputs_duplicate"),
        (&[][..], "outputs_empty"),
        (&["image"][..], "output_not_in_manifest"),
    ] {
        let plugin = plugin(&viewer_manifest(&["text"]), &offering(outputs, None, None));
        assert_eq!(bad_output_what(open(&plugin).await), what);
    }
}

#[tokio::test]
async fn a_plugin_cannot_claim_a_host_code() {
    let mut bed = Testbed::new();
    let claimed = bed.data(codes::PLUGIN_CRASHED.as_bytes());
    let plugin = plugin(
        &viewer_manifest(&["text"]),
        &bed.body(Export::Open, testbed::err_failure(8, claimed)),
    );
    for _ in 0..3 {
        let failure = open(&plugin).await.err().expect("its own failure");
        assert_eq!(
            failure.code, None,
            "the host's namespace is not the plugin's"
        );
    }
    assert!(
        !plugin.health().is_suspended(),
        "a plugin's own failure is no crash"
    );
}
