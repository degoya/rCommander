//! Which plugins claim a file, and in which order they are asked, decided without running one:
//! `[match]`, the associations before every manifest, the built-in formats first, an extension
//! before a magic, each by name, at most four (RC-100-27).

use std::{collections::BTreeMap, sync::Arc};

use rc_plugin_host::{ExecutionLog, testbed::Testbed};

use super::{MAX_CANDIDATES, ViewerPlugins};
use crate::plugins::test_support::{Spec, one, plugin};

const PNG_HEAD: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

/// `--- ` in hex: the start of a plain `diff -u`, and of a YAML document with a comment or tag.
const DASHES: &str = "2d2d2d20";

fn claimed(set: &ViewerPlugins, name: &str, head: &[u8], size: u64) -> Vec<String> {
    set.claims(name, head, size)
        .iter()
        .map(|viewer| viewer.name().to_owned())
        .collect()
}

fn associated(associations: &[(&str, &str)]) -> BTreeMap<String, String> {
    associations
        .iter()
        .map(|(extension, id)| ((*extension).to_owned(), (*id).to_owned()))
        .collect()
}

/// The plugins of `specs`, built from one test bed, with `associations`.
fn set_of(specs: &[Spec<'_>], associations: &[(&str, &str)]) -> ViewerPlugins {
    let log = Arc::new(ExecutionLog::new());
    let bed = Testbed::new();
    let plugins = specs.iter().map(|spec| plugin(spec, &bed, &log)).collect();
    ViewerPlugins::from_plugins(plugins, associated(associations), log)
}

/// Two plugins for `.csv`, Beta listed before Alpha; Beta also takes `.png`, `.md` and `%PDF`.
fn two(associations: &[(&str, &str)]) -> ViewerPlugins {
    set_of(
        &[
            Spec {
                id: "b2",
                name: "Beta",
                extensions: &["csv", "png", "md"],
                magic: Some("25504446"),
                max_size: Some(1000),
                ..Spec::default()
            },
            Spec {
                id: "a1",
                name: "Alpha",
                ..Spec::default()
            },
        ],
        associations,
    )
}

#[test]
fn an_extension_or_a_magic_claims_within_the_size_by_name() {
    let set = two(&[]);
    assert_eq!(
        claimed(&set, "data.CSV", b"a,b", 10),
        ["Alpha", "Beta"],
        "both by name, the extension in any case"
    );
    assert_eq!(
        claimed(&set, "data.csv", b"a,b", 1001),
        ["Alpha"],
        "too large for Beta"
    );
    assert_eq!(
        claimed(&set, "report", b"%PDF-1.7", 10),
        ["Beta"],
        "a magic claims a file without an extension"
    );
    assert!(
        claimed(&set, "report", b"%PDF-1.7", 1001).is_empty(),
        "too large"
    );
    assert!(claimed(&set, "notes.txt", b"plain", 10).is_empty());
}

#[test]
fn images_and_markdown_stay_built_in_unless_assigned() {
    let set = two(&[]);
    assert!(claimed(&set, "logo.png", PNG_HEAD, 10).is_empty());
    assert!(claimed(&set, "README.md", b"# Title", 10).is_empty());
    assert!(
        claimed(&set, "picture.csv", PNG_HEAD, 10).is_empty(),
        "an image by its bytes, whatever its name"
    );

    let beta = "019d0000-0000-7000-8000-0000000000b2";
    let assigned = two(&[("png", beta), ("csv", beta)]);
    assert_eq!(
        claimed(&assigned, "logo.png", PNG_HEAD, 10),
        ["Beta"],
        "assigned in the settings, before the built-in viewer, and nothing else"
    );
    assert_eq!(
        claimed(&assigned, "data.csv", b"a,b", 10),
        ["Beta", "Alpha"],
        "before the first by name, and only once"
    );
    assert_eq!(
        claimed(&assigned, "data.csv", b"a,b", 1001),
        ["Alpha"],
        "too large for its association, the file goes to the others"
    );
}

#[test]
fn an_association_to_a_plugin_not_installed_is_no_association() {
    let set = two(&[("csv", "019d0000-0000-7000-8000-0000000000ff")]);
    assert_eq!(claimed(&set, "data.csv", b"a,b", 10), ["Alpha", "Beta"]);
}

/// Five plugins, sorted by name: Alpha, Diff viewer and Omega by the magic `--- ` alone, YAML
/// viewer by its extensions, Zeta by `.yaml` and the magic.
fn five() -> [Spec<'static>; 5] {
    let by_magic = |id, name, extensions: &'static [&'static str]| Spec {
        id,
        name,
        extensions,
        magic: Some(DASHES),
        ..Spec::default()
    };
    [
        by_magic("c3", "Omega", &["omega"]),
        by_magic("a1", "Alpha", &["txt"]),
        Spec {
            id: "e5",
            name: "YAML viewer",
            extensions: &["yaml", "yml"],
            ..Spec::default()
        },
        by_magic("d4", "Diff viewer", &["diff", "patch", "rej"]),
        by_magic("f6", "Zeta", &["yaml"]),
    ]
}

#[test]
fn the_association_then_the_extensions_then_the_magics_each_by_name_at_most_four() {
    let set = set_of(&five(), &[]);
    let yaml = b"--- # compose\nservices: {}\n";
    assert_eq!(
        claimed(&set, "compose.yaml", yaml, 10),
        ["YAML viewer", "Zeta", "Alpha", "Diff viewer"],
        "an extension before a bare magic, Zeta once, Omega past the fourth"
    );
    assert_eq!(MAX_CANDIDATES, 4);
    assert_eq!(
        claimed(&set, "fix", b"--- a/x\n+++ b/x\n", 10),
        ["Alpha", "Diff viewer", "Omega", "Zeta"],
        "no extension: the magics by name"
    );
    assert_eq!(
        claimed(&set, "notes.txt", b"plain", 10),
        ["Alpha"],
        "an extension without the magic"
    );

    let omega = set_of(&five(), &[("yaml", "019d0000-0000-7000-8000-0000000000c3")]);
    assert_eq!(
        claimed(&omega, "compose.yaml", yaml, 10),
        ["Omega", "YAML viewer", "Zeta", "Alpha"],
        "the person's choice first"
    );
}

/// The bundled diff viewer has the magic `--- ` again (0.1.1): a YAML document starting so goes
/// to the YAML viewer first, a diff without an extension to the diff viewer.
#[test]
fn a_yaml_document_starting_with_dashes_goes_to_the_yaml_viewer_first() {
    let set = set_of(
        &[
            Spec {
                id: "d4",
                name: "Diff viewer",
                extensions: &["diff", "patch", "rej"],
                magic: Some(DASHES),
                ..Spec::default()
            },
            Spec {
                id: "e5",
                name: "YAML viewer",
                extensions: &["yaml", "yml"],
                ..Spec::default()
            },
        ],
        &[],
    );
    assert_eq!(
        claimed(&set, "docker-compose.yml", b"--- # app\nservices:\n", 10),
        ["YAML viewer", "Diff viewer"]
    );
    assert_eq!(
        claimed(&set, "docker-compose.yml", b"---\nservices:\n", 10),
        ["YAML viewer"],
        "`---` alone is no `--- `"
    );
    assert_eq!(
        claimed(&set, "fix", b"--- a/x\n+++ b/x\n@@ -1 +1 @@\n", 10),
        ["Diff viewer"]
    );
}

#[test]
fn the_first_of_an_id_wins_and_the_head_covers_every_magic() {
    let log = Arc::new(ExecutionLog::new());
    let bed = Testbed::new();
    let first = plugin(&Spec::default(), &bed, &log);
    let again = plugin(
        &Spec {
            name: "Another version",
            ..Spec::default()
        },
        &bed,
        &log,
    );
    let set = ViewerPlugins::from_plugins(vec![first, again], BTreeMap::new(), log);
    let names: Vec<&str> = set.iter().map(|viewer| viewer.name()).collect();
    assert_eq!(names, ["Test bed"]);
    assert_eq!(set.head_length(), 64, "the built-in image check's bytes");

    assert!(ViewerPlugins::none().is_empty());
    let magic = one(
        &Spec {
            magic: Some("25504446"),
            ..Spec::default()
        },
        &bed,
    );
    assert_eq!(magic.head_length(), 64);
}
