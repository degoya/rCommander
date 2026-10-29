use super::*;
use crate::Viewer;
use crate::outline::MAX_NODES;
use crate::test_support::{Content, TestSource, named};

fn open(bytes: &[u8], name: &str) -> Viewer<TestSource<Vec<u8>>> {
    Viewer::open(named(bytes, name), None)
        .expect("the document opens")
        .0
}

fn children<C: Content>(viewer: &mut Viewer<TestSource<C>>, node: Option<u64>) -> Vec<TreeNode> {
    viewer.children(node).expect("children")
}

/// Label and value of each node, the value `-` when it has none.
fn shown(nodes: &[TreeNode]) -> Vec<(String, String)> {
    nodes
        .iter()
        .map(|node| {
            (
                node.label.clone(),
                node.value.clone().unwrap_or_else(|| "-".to_owned()),
            )
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(label, value)| ((*label).to_owned(), (*value).to_owned()))
        .collect()
}

/// The child of `nodes` labelled `label`.
fn find<'n>(nodes: &'n [TreeNode], label: &str) -> &'n TreeNode {
    nodes
        .iter()
        .find(|node| node.label == label)
        .expect("the node is there")
}

#[test]
fn ini_keys_before_the_first_section_then_the_sections() {
    let text = "top = 1\n[server]\nhost = example.org ; main\nport = 8080\n\n[empty]\n; nothing\n[client]\nname=x\n";
    let mut viewer = open(text.as_bytes(), "app.ini");
    let top = children(&mut viewer, None);
    assert_eq!(
        shown(&top),
        pairs(&[
            ("top", "1"),
            ("server", "-"),
            ("empty", "-"),
            ("client", "-")
        ])
    );
    assert!(!top[0].has_children);
    assert!(top[1].has_children);
    assert!(!top[2].has_children);
    let server = children(&mut viewer, Some(top[1].id));
    assert_eq!(
        shown(&server),
        pairs(&[("host", "example.org"), ("port", "8080")])
    );
    assert!(children(&mut viewer, Some(server[0].id)).is_empty());
    assert!(children(&mut viewer, Some(top[2].id)).is_empty());
    assert_eq!(
        shown(&children(&mut viewer, Some(top[3].id))),
        pairs(&[("name", "x")])
    );
    let missing = viewer.children(Some(999)).expect_err("no such node");
    assert_eq!(missing.kind, FailureKind::NotFound);
}

#[test]
fn toml_tables_nest_and_arrays_of_tables_count_their_elements() {
    let text = "\
name = \"x\"
[package]
name = \"rc\"
version = \"0.1.0\"
[dependencies]
serde = { version = \"1\", features = [\"derive\"] }
list = [
  \"a\",
  \"b\",
]
[[bin]]
name = \"a\"
[[bin]]
name = \"b\"
[bin.extra]
k = 1
[target.'cfg(windows)'.dependencies]
winreg = \"0.5\"
";
    let mut viewer = open(text.as_bytes(), "Cargo.toml");
    let top = children(&mut viewer, None);
    assert_eq!(
        shown(&top),
        pairs(&[
            ("name", "\"x\""),
            ("package", "-"),
            ("dependencies", "-"),
            ("bin", "-"),
            ("target", "-")
        ])
    );
    let dependencies = children(&mut viewer, Some(find(&top, "dependencies").id));
    assert_eq!(
        shown(&dependencies),
        pairs(&[
            ("serde", "{ version = \"1\", features = [\"derive\"] }"),
            ("list", "[\u{2026}")
        ])
    );
    let bin = children(&mut viewer, Some(find(&top, "bin").id));
    assert_eq!(shown(&bin), pairs(&[("[0]", "-"), ("[1]", "-")]));
    let second = children(&mut viewer, Some(bin[1].id));
    assert_eq!(shown(&second), pairs(&[("name", "\"b\""), ("extra", "-")]));
    let extra = children(&mut viewer, Some(second[1].id));
    assert_eq!(shown(&extra), pairs(&[("k", "1")]));
    let target = children(&mut viewer, Some(find(&top, "target").id));
    assert_eq!(shown(&target), pairs(&[("cfg(windows)", "-")]));
    let cfg = children(&mut viewer, Some(target[0].id));
    let deps = children(&mut viewer, Some(cfg[0].id));
    assert_eq!(shown(&deps), pairs(&[("winreg", "\"0.5\"")]));
}

#[test]
fn a_registry_export_in_utf16_has_its_keys_as_sections() {
    let text = "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\Test]\r\n\"Name\"=\"Wert \u{e4}\"\r\n\"Count\"=dword:0000001f\r\n\"Bin\"=hex:01,02,\\\r\n  03\r\n\r\n[HKEY_CURRENT_USER\\Software\\Test\\Sub]\r\n@=\"x\"\r\n";
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    let mut viewer = open(&bytes, "export.reg");
    let top = children(&mut viewer, None);
    assert_eq!(
        shown(&top),
        pairs(&[
            ("HKEY_CURRENT_USER\\Software\\Test", "-"),
            ("HKEY_CURRENT_USER\\Software\\Test\\Sub", "-")
        ])
    );
    assert_eq!(
        shown(&children(&mut viewer, Some(top[0].id))),
        pairs(&[
            ("Name", "\"Wert \u{e4}\""),
            ("Count", "dword:0000001f"),
            ("Bin", "hex:01,02,\\\u{2026}")
        ])
    );
    assert_eq!(
        shown(&children(&mut viewer, Some(top[1].id))),
        pairs(&[("@", "\"x\"")])
    );
}

#[test]
fn env_and_properties_are_flat() {
    let mut env = open(b"A=1\nexport B=\"two\nlines\"\n# c\nC=3\n", ".env");
    assert_eq!(
        shown(&children(&mut env, None)),
        pairs(&[("A", "1"), ("B", "\"two\u{2026}"), ("C", "3")])
    );
    let mut properties = open(
        b"greeting = Hello \\\n    World\ncount: 2\n! comment\nflag\n",
        "app.properties",
    );
    assert_eq!(
        shown(&children(&mut properties, None)),
        pairs(&[
            ("greeting", "Hello \\\u{2026}"),
            ("count", "2"),
            ("flag", "-")
        ])
    );
    let mut ignore = open(b"# build output\ntarget/\n!keep.log\n", ".gitignore");
    assert_eq!(
        shown(&children(&mut ignore, None)),
        pairs(&[("target/", "-"), ("!keep.log", "-")])
    );
}

#[test]
fn many_entries_are_shown_in_ranges() {
    let text: String = (0..2500)
        .map(|index| format!("K{index}={index}\n"))
        .collect();
    let mut viewer = open(text.as_bytes(), ".env");
    let top = children(&mut viewer, None);
    assert_eq!(
        shown(&top),
        pairs(&[
            ("items 1..1000", "-"),
            ("items 1001..2000", "-"),
            ("items 2001..2500", "-")
        ])
    );
    let second = children(&mut viewer, Some(top[1].id));
    assert_eq!(second.len(), 1000);
    assert_eq!(
        (second[0].label.as_str(), second[0].value.as_deref()),
        ("K1000", Some("1000"))
    );
    assert_eq!(second[999].label, "K1999");
    // The same range twice is the same node.
    assert_eq!(children(&mut viewer, None)[1].id, top[1].id);
    let last = children(&mut viewer, Some(top[2].id));
    assert_eq!(last.len(), 500);
    assert_eq!(last[499].label, "K2499");
}

#[test]
fn sections_past_the_limit_are_named_not_shown() {
    let text = "[s]\nk=1\n".repeat(MAX_NODES + 3);
    let mut viewer = open(text.as_bytes(), "big.ini");
    let top = children(&mut viewer, None);
    assert_eq!(top.len(), 17);
    let last = children(&mut viewer, Some(top[16].id));
    assert_eq!(last.len(), MAX_NODES - 16_000 + 1);
    let marker = last.last().expect("the marker");
    assert_eq!(marker.label, "3 more sections not shown");
    assert!(!marker.has_children);
    assert!(children(&mut viewer, Some(marker.id)).is_empty());
    let kept = &last[last.len() - 2];
    assert_eq!(
        shown(&children(&mut viewer, Some(kept.id))),
        pairs(&[("k", "1")])
    );
}
