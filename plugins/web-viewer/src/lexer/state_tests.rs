use super::*;

fn js(end: End) -> Frame {
    Frame::Js(JsFrame::new(Dialect::Js, end))
}

#[test]
fn the_stack_holds_64_frames_and_refuses_the_65th() {
    let mut stack = Stack::new(js(End::Never));
    for _ in 1..MAX_DEPTH {
        assert!(stack.push(Frame::Template(Dialect::Js)));
    }
    assert_eq!(stack.len(), MAX_DEPTH);
    assert!(!stack.push(js(End::Brace)));
    assert_eq!(stack.len(), MAX_DEPTH);
    assert_eq!(stack.top(), Frame::Template(Dialect::Js));
}

#[test]
fn the_bottom_frame_is_never_closed() {
    let mut stack = Stack::new(Frame::Html(HtmlSub::Text));
    stack.pop();
    stack.truncate(0);
    assert_eq!(stack.len(), 1);
    assert_eq!(stack.top(), Frame::Html(HtmlSub::Text));
}

#[test]
fn the_boundary_is_the_topmost_frame_a_quote_or_a_tag_ends() {
    let mut stack = Stack::new(Frame::Html(HtmlSub::Text));
    assert_eq!(stack.boundary(), None);
    stack.push(js(End::ScriptTag));
    stack.push(Frame::Template(Dialect::Js));
    stack.push(js(End::Brace));
    assert_eq!(stack.boundary(), Some((1, End::ScriptTag)));
    stack.truncate(1);
    assert_eq!(stack.len(), 1);
    stack.push(Frame::Css(CssFrame::new(
        CssDialect::Css,
        End::Quote(b'"'),
        1,
    )));
    assert_eq!(stack.boundary(), Some((1, End::Quote(b'"'))));
}

#[test]
fn a_stack_is_rebuilt_from_its_frames() {
    let mut stack = Stack::new(Frame::Html(HtmlSub::Text));
    stack.push(js(End::ScriptTag));
    stack.push(Frame::Template(Dialect::Ts));
    assert_eq!(Stack::of(stack.frames()), stack);
}

#[test]
fn a_marker_hash_tells_markers_apart() {
    assert_eq!(fnv(b"EOT"), fnv(b"EOT"));
    assert_ne!(fnv(b"EOT"), fnv(b"EOF"));
}
