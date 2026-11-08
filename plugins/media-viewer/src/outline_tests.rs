use super::*;
use crate::test_support::VecSink;

fn sample() -> Outline {
    Outline::new(vec![
        Node::group(
            "Audio",
            None,
            vec![
                Node::fact("Format", "FLAC"),
                Node::fact("Duration", "0:10.000"),
            ],
        ),
        Node::group(
            "Tracks",
            None,
            vec![Node::group(
                "Track 1",
                Some("Video".to_owned()),
                vec![Node::fact("Codec", "AV1")],
            )],
        ),
    ])
}

#[test]
fn the_text_is_indented_label_value_lines() {
    assert_eq!(
        sample().render(),
        "Audio\n  Format: FLAC\n  Duration: 0:10.000\n\nTracks\n  Track 1: Video\n    Codec: AV1\n"
    );
}

#[test]
fn children_come_a_level_at_a_time() {
    let outline = sample();
    let roots = outline.children(None).expect("roots");
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].label, "Audio");
    assert!(roots[0].has_children);
    let facts = outline.children(Some(roots[0].id)).expect("facts");
    assert_eq!(facts[1].label, "Duration");
    assert_eq!(facts[1].value.as_deref(), Some("0:10.000"));
    assert!(!facts[1].has_children);
    assert!(outline.children(Some(10_000)).is_err());
}

#[test]
fn values_lose_control_characters_and_length() {
    assert_eq!(clean("two\nlines\0"), "two lines");
    let long = "x".repeat(MAX_VALUE_CHARS + 10);
    let cleaned = clean(&long);
    assert_eq!(cleaned.chars().count(), MAX_VALUE_CHARS + 3);
    assert!(cleaned.ends_with("..."));
}

#[test]
fn the_tree_is_capped() {
    let facts = (0..MAX_NODES + 50)
        .map(|index| Node::fact(index.to_string(), "v"))
        .collect();
    let outline = Outline::new(vec![Node::group("Many", None, facts)]);
    assert_eq!(outline.len(), MAX_NODES);
    assert!(!outline.is_empty());
}

#[test]
fn bytes_are_written_in_windows() {
    let bytes = vec![7u8; (1 << 20) + 5];
    let mut sink = VecSink::default();
    write_all(&mut sink, &bytes).expect("written");
    assert_eq!(sink.0, bytes);
}
