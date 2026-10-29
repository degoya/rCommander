use super::*;

fn labels(outline: &Outline, nodes: &[u32]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| outline.nodes[*node as usize].label.clone())
        .collect()
}

#[test]
fn ini_sections_are_one_level_in_file_order() {
    let mut builder = OutlineBuilder::new(false);
    builder.header(false, "b", 2);
    builder.header(false, "a", 5);
    builder.header(false, "b", 5);
    builder.header(false, "", 9);
    let outline = builder.finish(9);
    assert_eq!(outline.root_entries, 2);
    assert_eq!(labels(&outline, &outline.top), ["b", "a", "b", "[]"]);
    let entries: Vec<(u64, u64)> = outline
        .nodes
        .iter()
        .map(|node| (node.first_entry, node.entries))
        .collect();
    assert_eq!(entries, [(2, 3), (5, 0), (5, 4), (9, 0)]);
}

#[test]
fn without_a_header_every_entry_is_the_top_levels() {
    let outline = OutlineBuilder::new(true).finish(7);
    assert_eq!(outline.root_entries, 7);
    assert!(outline.top.is_empty());
}

#[test]
fn toml_tables_nest_by_their_dots() {
    let mut builder = OutlineBuilder::new(true);
    // `a` made by `a.b`, then named itself.
    builder.header(false, "a.b", 1);
    builder.header(false, "a", 2);
    builder.header(true, "bin", 3);
    builder.header(true, "bin", 4);
    // Belongs to the last `[[bin]]`.
    builder.header(false, "bin.extra", 6);
    builder.header(false, "x.\"y.z\"", 7);
    let outline = builder.finish(8);
    assert_eq!(outline.root_entries, 1);
    assert_eq!(labels(&outline, &outline.top), ["a", "bin", "x"]);
    let a = &outline.nodes[outline.top[0] as usize];
    assert_eq!((a.first_entry, a.entries), (2, 1));
    assert_eq!(labels(&outline, &a.children), ["b"]);
    let b = &outline.nodes[a.children[0] as usize];
    assert_eq!((b.first_entry, b.entries), (1, 1));
    let bin = &outline.nodes[outline.top[1] as usize];
    assert_eq!(bin.entries, 0);
    assert_eq!(labels(&outline, &bin.children), ["[0]", "[1]"]);
    let second = &outline.nodes[bin.children[1] as usize];
    assert_eq!((second.first_entry, second.entries), (4, 2));
    assert_eq!(labels(&outline, &second.children), ["extra"]);
    let x = &outline.nodes[outline.top[2] as usize];
    assert_eq!(labels(&outline, &x.children), ["y.z"]);
}

#[test]
fn key_paths_take_their_quotes_off() {
    assert_eq!(key_path("a.b"), ["a", "b"]);
    assert_eq!(key_path(" a . \"b.c\" . 'd e' "), ["a", "b.c", "d e"]);
    assert_eq!(key_path("\"a\\\"b\""), ["a\\\"b"]);
    let deep = vec!["x"; 40].join(".");
    let parts = key_path(&deep);
    assert_eq!(parts.len(), MAX_SEGMENTS);
    assert_eq!(parts[MAX_SEGMENTS - 1], ["x"; 9].join("."));
    let long = "\u{e4}".repeat(200);
    assert!(key_path(&long)[0].len() <= MAX_SEGMENT_BYTES);
}

#[test]
fn nodes_past_the_limit_are_counted_not_kept() {
    let mut builder = OutlineBuilder::new(false);
    for index in 0..MAX_NODES as u64 + 5 {
        builder.header(false, "s", index);
    }
    let outline = builder.finish(MAX_NODES as u64 + 5);
    assert_eq!(outline.nodes.len(), MAX_NODES);
    assert_eq!(outline.hidden, 5);
    assert_eq!(outline.nodes[MAX_NODES - 1].entries, 1);
    assert!(outline.memory_bytes() < 4 << 20);
}
