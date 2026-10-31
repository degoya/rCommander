//! Every installation kind from a path fixture, and what each one is told to do (RC-100-06,
//! RC-100-52 ... RC-100-54; after rDownloader's `install_kind_tests.rs`).

use super::*;
use crate::install::fixture::write;

/// [`detect_with`] for a program that is where it was started, without an environment's kind
/// and without Homebrew.
fn detect_here(executable: &Path) -> Installation {
    detect_with(executable, executable, None, &[])
}

/// The program at `relative` under `root`, written.
fn program(root: &Path, relative: &[&str]) -> PathBuf {
    let path = relative
        .iter()
        .fold(root.to_owned(), |path, part| path.join(part));
    write(&path, "program");
    path
}

#[test]
fn the_unpacked_package_is_portable() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander.exe");
    write(&exe, "program");
    write(&root.path().join("VERSION.txt"), "rCommander 1.0.0");
    let found = detect_here(&exe);
    assert_eq!(found.kind, InstallKind::Portable);
    assert_eq!(found.reason, None);
    assert_eq!(found.install_dir, root.path());
    assert_eq!(found.executable, "rcommander.exe");
}

/// A build from source and the copy of `scripts/run-windows.sh` carry no `VERSION.txt`.
#[test]
fn a_program_without_its_version_file_is_unknown() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander.exe");
    write(&exe, "program");
    let found = detect_here(&exe);
    assert_eq!(found.kind, InstallKind::Unknown);
    assert_eq!(found.reason, Some("update.kind_no_version_file"));
}

#[test]
fn a_marker_of_an_unknown_kind_makes_it_unknown_and_portable_keeps_it() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander.exe");
    write(&exe, "program");
    write(&root.path().join("VERSION.txt"), "rCommander 1.0.0");
    write(
        &root.path().join(INSTALL_KIND_FILE),
        "# written by the installer\nmsi\n",
    );
    assert_eq!(detect_here(&exe).reason, Some("update.kind_marker"));
    write(&root.path().join(INSTALL_KIND_FILE), "portable\n");
    assert_eq!(detect_here(&exe).kind, InstallKind::Portable);
}

#[test]
fn a_macos_bundle_is_installed_as_its_folder_entry() {
    let root = tempfile::tempdir().expect("tempdir");
    let app = root.path().join("rCommander.app");
    let exe = app.join("Contents").join("MacOS").join("rcommander");
    write(&exe, "program");
    write(
        &app.join("Contents").join("Resources").join("VERSION.txt"),
        "rCommander 1.0.0",
    );
    let found = detect_here(&exe);
    assert_eq!(found.kind, InstallKind::Portable);
    assert_eq!(found.install_dir, root.path());
    assert_eq!(found.executable, "rCommander.app/Contents/MacOS/rcommander");
}

#[cfg(unix)]
#[test]
fn a_folder_this_process_cannot_write_is_unknown() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("tempdir");
    let exe = root.path().join("rcommander");
    write(&exe, "program");
    write(&root.path().join("VERSION.txt"), "rCommander 1.0.0");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o555)).expect("lock");
    let probe_writes = fs::write(root.path().join("probe"), "").is_ok();
    let found = detect_here(&exe);
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).expect("unlock");
    if probe_writes {
        // Permissions do not hold for this user (root); nothing to prove.
        return;
    }
    assert_eq!(found.reason, Some("update.kind_not_writable"));
}

#[test]
fn the_environment_names_a_managers_kind_before_anything_else() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = program(
        root.path(),
        &["scoop", "apps", "rcommander", "1.0.0", "rcommander.exe"],
    );
    let found = detect_with(&exe, &exe, Some("RPM"), &[]);
    assert_eq!(found.kind, InstallKind::Rpm);
    assert_eq!(found.reason, Some("update.kind_package_manager"));
    // A value that names nothing is ignored rather than trusted.
    assert_eq!(
        detect_with(&exe, &exe, Some("nonsense"), &[]).kind,
        InstallKind::Scoop
    );
    // `portable` skips the layout, but not the version file.
    assert_eq!(
        detect_with(&exe, &exe, Some("portable"), &[]).reason,
        Some("update.kind_no_version_file")
    );
}

/// Scoop, Homebrew and winget install the portable archive, `VERSION.txt` and all: the layout
/// wins, on the resolved path.
#[test]
fn a_package_manager_layout_wins_over_the_archives_files() {
    let root = tempfile::tempdir().expect("tempdir");
    let scoop = program(
        root.path(),
        &[
            "Users",
            "me",
            "scoop",
            "apps",
            "rcommander",
            "current",
            "rcommander.exe",
        ],
    );
    write(&scoop.with_file_name("VERSION.txt"), "rCommander 1.0.0");
    write(&scoop.with_file_name(INSTALL_KIND_FILE), "portable\n");
    assert_eq!(detect_here(&scoop).kind, InstallKind::Scoop);

    let cellar = program(
        root.path(),
        &[
            "home",
            "linuxbrew",
            ".linuxbrew",
            "Cellar",
            "rcommander",
            "1.0.0",
            "libexec",
            "rcommander",
        ],
    );
    write(&cellar.with_file_name("VERSION.txt"), "rCommander 1.0.0");
    let link = root.path().join("bin").join("rcommander");
    // The formula's `bin/rcommander` resolves into the Cellar.
    assert_eq!(
        detect_with(&link, &cellar, None, &[]).kind,
        InstallKind::Homebrew
    );

    for links in ["Packages", "Links"] {
        let winget = program(
            root.path(),
            &[
                "Users",
                "me",
                "AppData",
                "Local",
                "Microsoft",
                "WinGet",
                links,
                "degoya.rCommander_Microsoft.Winget.Source_8wekyb3d8bbwe",
                "rcommander.exe",
            ],
        );
        let found = detect_here(&winget);
        assert_eq!(found.kind, InstallKind::Winget, "{links}");
        assert_eq!(found.reason, Some("update.kind_package_manager"));
    }
}

/// A folder of one's own called `apps` is not Scoop.
#[test]
fn an_apps_folder_outside_scoop_is_portable() {
    let root = tempfile::tempdir().expect("tempdir");
    let exe = program(
        root.path(),
        &["Tools", "apps", "rcommander", "current", "rcommander.exe"],
    );
    write(&exe.with_file_name("VERSION.txt"), "rCommander 1.0.0");
    assert_eq!(detect_here(&exe).kind, InstallKind::Portable);
}

/// The cask moves the `.app` into `/Applications`; Homebrew's Caskroom names it.
#[test]
fn a_bundle_with_a_caskroom_entry_is_homebrew() {
    let root = tempfile::tempdir().expect("tempdir");
    let app = root.path().join("Applications").join("rCommander.app");
    let exe = app.join("Contents").join("MacOS").join("rcommander");
    write(&exe, "program");
    write(
        &app.join("Contents").join("Resources").join("VERSION.txt"),
        "rCommander 1.0.0",
    );
    let prefix = root.path().join("opt").join("homebrew");
    assert_eq!(
        detect_with(&exe, &exe, None, std::slice::from_ref(&prefix)).kind,
        InstallKind::Portable
    );
    fs::create_dir_all(prefix.join("Caskroom").join("rcommander").join("1.0.0")).expect("cask");
    let found = detect_with(&exe, &exe, None, std::slice::from_ref(&prefix));
    assert_eq!(found.kind, InstallKind::Homebrew);
    assert_eq!(found.executable, "rCommander.app/Contents/MacOS/rcommander");
    // Started as the cask's command, a link in `<prefix>/bin` into the bundle.
    let link = prefix.join("bin").join("rcommander");
    assert_eq!(
        detect_with(&link, &exe, None, &[prefix]).kind,
        InstallKind::Homebrew
    );
}

/// The deb and rpm keep `/usr/bin` clean: the marker is `/usr/lib/rcommander/install-kind`,
/// beside the real program and reached from the link's `bin` folder too.
#[test]
fn a_distribution_package_is_named_by_its_marker() {
    for (content, expected) in [("deb\n", InstallKind::Deb), ("rpm\n", InstallKind::Rpm)] {
        let root = tempfile::tempdir().expect("tempdir");
        let real = program(root.path(), &["usr", "lib", "rcommander", "rcommander"]);
        write(&real.with_file_name("VERSION.txt"), "rCommander 1.0.0");
        write(&real.with_file_name(INSTALL_KIND_FILE), content);
        assert_eq!(detect_here(&real).kind, expected, "{content:?}");
        let link = root.path().join("usr").join("bin").join("rcommander");
        assert_eq!(
            detect_with(&link, &link, None, &[]).kind,
            expected,
            "{content:?}"
        );
    }
}

/// Path beats marker, marker beats `VERSION.txt`, and only the first KiB of a marker is read.
#[test]
fn the_order_of_the_rules_and_the_markers_size() {
    let root = tempfile::tempdir().expect("tempdir");
    let scoop = program(
        root.path(),
        &["scoop", "apps", "rcommander", "1.0.0", "rcommander.exe"],
    );
    write(&scoop.with_file_name(INSTALL_KIND_FILE), "deb\n");
    assert_eq!(detect_here(&scoop).kind, InstallKind::Scoop);

    let plain = program(root.path(), &["Tools", "rCommander", "rcommander.exe"]);
    write(&plain.with_file_name("VERSION.txt"), "rCommander 1.0.0");
    write(&plain.with_file_name(INSTALL_KIND_FILE), "winget\n");
    assert_eq!(detect_here(&plain).kind, InstallKind::Winget);

    // The kind after 1 KiB of comments is never read: no marker, so `VERSION.txt` decides.
    let padding = "#".repeat(1100);
    write(
        &plain.with_file_name(INSTALL_KIND_FILE),
        &format!("{padding}\nwinget\n"),
    );
    assert_eq!(detect_here(&plain).kind, InstallKind::Portable);
}

#[test]
fn every_kind_round_trips_through_its_name() {
    for kind in [
        InstallKind::Portable,
        InstallKind::Scoop,
        InstallKind::Homebrew,
        InstallKind::Winget,
        InstallKind::Deb,
        InstallKind::Rpm,
    ] {
        assert_eq!(InstallKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(InstallKind::parse("unknown"), None);
}

#[test]
fn each_manager_has_its_command_and_only_portable_installs_itself() {
    assert!(InstallKind::Portable.installs_itself());
    assert_eq!(InstallKind::Portable.update_command(), None);
    assert_eq!(InstallKind::Unknown.update_command(), None);
    for (kind, command) in [
        (InstallKind::Scoop, "scoop update rcommander"),
        (InstallKind::Homebrew, "brew upgrade rcommander"),
        (InstallKind::Winget, "winget upgrade degoya.rCommander"),
        (
            InstallKind::Deb,
            "sudo apt update && sudo apt install --only-upgrade rcommander",
        ),
        (InstallKind::Rpm, "sudo dnf upgrade --refresh rcommander"),
    ] {
        assert_eq!(kind.update_command(), Some(command), "{kind:?}");
        assert!(kind.manager_name().is_some(), "{kind:?}");
        assert!(!kind.installs_itself(), "{kind:?}");
        assert!(!kind.receives_betas(), "{kind:?}");
    }
    assert!(InstallKind::Portable.receives_betas());
}

#[test]
fn the_windows_verbatim_prefix_is_dropped() {
    assert_eq!(
        strip_verbatim(Path::new(
            r"\\?\C:\scoop\apps\rcommander\1.0.0\rcommander.exe"
        )),
        PathBuf::from(r"C:\scoop\apps\rcommander\1.0.0\rcommander.exe")
    );
}
