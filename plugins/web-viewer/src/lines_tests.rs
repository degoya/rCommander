use super::*;
use crate::lexer::state::{Dialect, End, HtmlSub, JsFrame, MAX_DEPTH};

fn html() -> LineState {
    LineState::new(Frame::Html(HtmlSub::Text))
}

/// HTML with a script whose template string is open.
fn nested() -> LineState {
    let mut state = html();
    state
        .stack
        .push(Frame::Js(JsFrame::new(Dialect::Ts, End::ScriptTag)));
    state.stack.push(Frame::Template(Dialect::Ts));
    state
}

#[test]
fn a_checkpoint_every_64_lines_or_after_a_long_line() {
    let mut index = LineIndex::new(0, &html());
    for line in 1..200u64 {
        // Line 100 is 100 KiB long: the line after it gets a checkpoint of its own.
        let offset = line * 10 + if line > 100 { 100 * 1024 } else { 0 };
        index.line_start(line, offset, &html());
    }
    index.finish(200);
    assert_eq!(index.lines(), 200);
    assert_eq!(index.at_or_before(63).line, 0);
    assert_eq!(index.at_or_before(64).line, 64);
    assert_eq!(index.at_or_before(100).line, 64);
    assert_eq!(index.at_or_before(150).line, 101);
    assert_eq!(index.exact(101).map(|point| point.line), Some(101));
    assert_eq!(index.exact(102), None);
}

#[test]
fn a_checkpoint_gives_back_the_state_it_kept() {
    let mut index = LineIndex::new(0, &html());
    index.line_start(64, 640, &nested());
    let mut php = html();
    php.php = Some(PhpSub::Str(b'"'));
    index.line_start(128, 1280, &php);
    index.finish(200);
    assert_eq!(index.state(&index.at_or_before(10)), html());
    assert_eq!(index.state(&index.at_or_before(70)), nested());
    assert_eq!(index.state(&index.at_or_before(199)), php);
}

#[test]
fn the_index_thins_itself_and_stays_bounded() {
    let state = html();
    let mut index = LineIndex::new(3, &state);
    let lines = 64 * MAX_CHECKPOINTS as u64 * 5;
    for line in 1..lines {
        index.line_start(line, 3 + line * 40, &state);
    }
    index.finish(lines);
    assert!(index.points.len() < MAX_CHECKPOINTS);
    assert_eq!(index.at_or_before(0).offset, 3);
    // Every checkpoint left is still where its line starts.
    assert!(
        index
            .points
            .iter()
            .all(|point| point.offset == 3 + point.line * 40)
    );
    let last = index.at_or_before(lines - 1);
    assert!(lines - 1 - last.line < index.every_lines);
}

#[test]
fn deep_states_thin_the_index_by_their_frames() {
    let mut deep = html();
    while deep.stack.len() < MAX_DEPTH {
        deep.stack.push(Frame::Template(Dialect::Js));
    }
    let mut index = LineIndex::new(0, &deep);
    let lines = 64 * (MAX_FRAMES / MAX_DEPTH) as u64 * 4;
    for line in 1..lines {
        index.line_start(line, line * 40, &deep);
    }
    index.finish(lines);
    assert!(index.frames.len() <= MAX_FRAMES);
    assert!(index.memory_bytes() <= crate::INDEX_BUDGET_BYTES);
    let point = index.at_or_before(lines / 2);
    assert_eq!(index.state(&point), deep);
}
