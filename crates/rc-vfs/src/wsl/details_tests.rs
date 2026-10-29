use std::path::PathBuf;

use super::*;

fn entry(guid: &str, name: &str, version: Option<u32>) -> LxssEntry {
    LxssEntry {
        guid: guid.to_owned(),
        name: name.to_owned(),
        version,
    }
}

fn lxss() -> Vec<LxssEntry> {
    vec![
        entry("{0b1c2d3e-0000-4000-8000-000000000001}", "Debian", Some(1)),
        entry("{0B1C2D3E-0000-4000-8000-000000000002}", "Ubuntu", Some(2)),
        entry("{0b1c2d3e-0000-4000-8000-000000000003}", "Odd", Some(7)),
    ]
}

#[test]
fn the_registry_default_is_found_by_its_guid_ignoring_case_and_braces() {
    let details = from_registry(Some("0b1c2d3e-0000-4000-8000-000000000002"), &lxss());
    assert_eq!(details.default.as_deref(), Some("Ubuntu"));
    let braced = from_registry(Some("{0B1C2D3E-0000-4000-8000-000000000001}"), &lxss());
    assert_eq!(braced.default.as_deref(), Some("Debian"));
}

#[test]
fn the_registry_versions_are_one_or_two() {
    let details = from_registry(None, &lxss());
    assert_eq!(details.default, None);
    assert_eq!(
        details.versions,
        [("Debian".to_owned(), 1), ("Ubuntu".to_owned(), 2)]
    );
}

#[test]
fn a_default_naming_no_subkey_marks_nothing() {
    let details = from_registry(Some("{ffffffff-0000-4000-8000-000000000000}"), &lxss());
    assert_eq!(details.default, None);
}

#[test]
fn the_default_is_marked_and_each_root_is_a_share() {
    let details = from_registry(Some("{0b1c2d3e-0000-4000-8000-000000000002}"), &lxss());
    let names = vec![
        "ubuntu".to_owned(),
        "Debian".to_owned(),
        "Alpine".to_owned(),
    ];
    let distros = assemble(names, &details, host_for_build(Some(22631)));
    assert_eq!(distros.len(), 3);
    assert_eq!(distros[0].name, "ubuntu");
    assert!(distros[0].is_default);
    assert_eq!(distros[0].version, Some(2));
    assert_eq!(
        distros[0].root,
        VfsPath::local(PathBuf::from(r"\\wsl.localhost\ubuntu\"))
    );
    assert!(!distros[1].is_default);
    assert_eq!(distros[1].version, Some(1));
    assert_eq!(distros[2].version, None);
    assert_eq!(distros.iter().filter(|distro| distro.is_default).count(), 1);
}

#[test]
fn old_builds_and_an_unknown_build_take_the_legacy_host() {
    assert_eq!(host_for_build(Some(19045)), "wsl$");
    assert_eq!(host_for_build(None), "wsl$");
    assert_eq!(host_for_build(Some(21354)), "wsl.localhost");
    assert_eq!(host_for_build(Some(26100)), "wsl.localhost");
    let distros = assemble(vec!["Ubuntu".to_owned()], &Details::default(), "wsl$");
    assert_eq!(distros[0].root.path, PathBuf::from(r"\\wsl$\Ubuntu\"));
}

#[test]
fn a_root_parses_back_to_its_distribution() {
    let distros = assemble(
        vec!["Ubuntu-24.04".to_owned()],
        &Details::default(),
        host_for_build(Some(22631)),
    );
    let parsed = crate::parse_wsl_unc(&distros[0].root.path).expect("a WSL share");
    assert_eq!(parsed.distro, "Ubuntu-24.04");
    assert_eq!(parsed.linux_path, "/");
}
