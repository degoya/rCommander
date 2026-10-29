use std::{collections::BTreeSet, path::Path};

use serde_json::Value;

use super::{Package, THIRD_PARTY, counts, matching, packages, summary};

const REGENERATE: &str = "run scripts/licenses.sh";

/// Read at run time, not with `include_str!`: a test binary can outlive the checkout that built
/// it when worktrees share one target directory.
fn workspace_file(relative: &str) -> String {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets the manifest dir");
    let path = Path::new(&manifest).join("../..").join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// The whole generated file, both lists.
fn file() -> Value {
    serde_json::from_str(THIRD_PARTY)
        .unwrap_or_else(|error| panic!("third-party.json: {error}; {REGENERATE}"))
}

/// `name@version` of every package in `Cargo.lock` that comes from somewhere - a registry or
/// git. Workspace members carry no `source` line and are rCommander itself.
fn locked_crates() -> BTreeSet<String> {
    let mut crates = BTreeSet::new();
    for block in workspace_file("Cargo.lock").split("[[package]]").skip(1) {
        let field = |key: &str| {
            block.lines().find_map(|line| {
                line.strip_prefix(key)
                    .and_then(|rest| rest.strip_prefix(" = \""))
                    .and_then(|rest| rest.strip_suffix('"'))
            })
        };
        if field("source").is_some() {
            let name = field("name").expect("a locked package has a name");
            let version = field("version").expect("a locked package has a version");
            crates.insert(format!("{name}@{version}"));
        }
    }
    crates
}

#[test]
fn the_list_reads_and_has_only_its_two_parts() {
    let file = file();
    let mut keys: Vec<&str> = file
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["rust", "rust_not_shipped"], "{REGENERATE}");
    assert_eq!(
        packages().len(),
        file["rust"].as_array().map_or(0, Vec::len)
    );
    assert!(!packages().is_empty(), "the page would list nothing");
}

/// The acceptance criterion: every crate of `Cargo.lock` is listed with its licence, or known
/// not to ship.
#[test]
fn every_locked_crate_is_listed_or_known_not_to_ship() {
    let file = file();
    let mut accounted: BTreeSet<String> = packages()
        .iter()
        .map(|package| format!("{}@{}", package.name, package.version))
        .collect();
    for entry in file["rust_not_shipped"].as_array().expect("a list") {
        accounted.insert(entry.as_str().expect("name@version").to_owned());
    }
    let locked = locked_crates();
    assert!(!locked.is_empty(), "Cargo.lock names no crate at all");
    let missing: Vec<_> = locked.difference(&accounted).collect();
    let stale: Vec<_> = accounted.difference(&locked).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "not listed: {missing:?}; listed but no longer locked: {stale:?}; {REGENERATE}"
    );
}

#[test]
fn every_listed_crate_names_its_licence() {
    for package in packages() {
        assert!(
            !package.license.trim().is_empty(),
            "{}@{} has no licence; record it in crates/rcommander/resources/licenses/\
             overrides.json with its source, then {REGENERATE}",
            package.name,
            package.version
        );
    }
}

#[test]
fn the_counts_add_up_most_common_first() {
    let total: usize = counts().iter().map(|(_, count)| count).sum();
    assert_eq!(total, packages().len());
    let package = |name: &str, license: &str| Package {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        license: license.to_owned(),
    };
    let list = [
        package("a", "MIT"),
        package("b", "Apache-2.0"),
        package("c", "MIT"),
        package("d", "BSD-3-Clause"),
    ];
    assert_eq!(
        summary(&list),
        [("MIT", 2), ("Apache-2.0", 1), ("BSD-3-Clause", 1)]
    );
}

#[test]
fn the_filter_matches_name_or_licence_ignoring_case() {
    assert_eq!(matching("").count(), packages().len());
    assert_eq!(matching("  ").count(), packages().len());
    let iced: Vec<&Package> = matching("ICED").collect();
    assert!(
        iced.iter().any(|package| package.name == "iced"),
        "{iced:?}"
    );
    let mit = matching("mit").count();
    assert!(mit > 0 && mit < packages().len(), "{mit}");
    assert_eq!(matching("no such crate or licence").count(), 0);
}
