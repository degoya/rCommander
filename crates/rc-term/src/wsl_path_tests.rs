use std::path::Path;

use super::{
    HomeReason, ShellPaths, WslCwd, panel_to_shell_path, shell_fits, shell_to_panel_path,
    windows_to_wsl_path,
};
use crate::Platform;

fn path(linux: &str) -> WslCwd {
    WslCwd::Path(linux.to_owned())
}

fn home(reason: HomeReason) -> WslCwd {
    WslCwd::Home(reason)
}

#[test]
fn windows_paths_translate_for_the_target_distribution() {
    let cases: &[(&str, Option<&str>, WslCwd)] = &[
        // Drives: the letter in lower case, the separators turned.
        (
            r"C:\Users\x\Documents",
            Some("Ubuntu"),
            path("/mnt/c/Users/x/Documents"),
        ),
        (r"D:\", Some("Ubuntu"), path("/mnt/d")),
        (r"e:", None, path("/mnt/e")),
        (r"C:\Users\x\", None, path("/mnt/c/Users/x")),
        ("C:/Users/x/src", None, path("/mnt/c/Users/x/src")),
        (r"\\?\C:\very\long", None, path("/mnt/c/very/long")),
        // Spaces and umlauts pass as they are; wsl.exe gets them as one argument.
        (
            r"C:\Program Files\My Folder",
            None,
            path("/mnt/c/Program Files/My Folder"),
        ),
        (
            "C:\\Users\\J\u{fc}rgen\\\u{dc}bersicht",
            None,
            path("/mnt/c/Users/J\u{fc}rgen/\u{dc}bersicht"),
        ),
        // The distribution's own share, under either host and ignoring case.
        (
            r"\\wsl.localhost\Ubuntu\home\x\src",
            Some("Ubuntu"),
            path("/home/x/src"),
        ),
        (r"\\wsl$\Ubuntu\home\x", Some("Ubuntu"), path("/home/x")),
        (r"\\WSL.LOCALHOST\ubuntu\etc", Some("Ubuntu"), path("/etc")),
        (r"\\wsl.localhost\Ubuntu\", Some("Ubuntu"), path("/")),
        (r"\\wsl.localhost\Ubuntu", Some("Ubuntu"), path("/")),
        (
            r"\\?\UNC\wsl.localhost\Ubuntu\home\x",
            Some("Ubuntu"),
            path("/home/x"),
        ),
        (
            "\\\\wsl.localhost\\Ubuntu\\home\\x\\Gr\u{fc}\u{df}e und mehr",
            Some("Ubuntu"),
            path("/home/x/Gr\u{fc}\u{df}e und mehr"),
        ),
        // Another distribution's share, or a share when the target is not known.
        (
            r"\\wsl.localhost\Ubuntu\home\x\src",
            Some("Debian"),
            home(HomeReason::OtherDistro),
        ),
        (
            r"\\wsl$\Debian\home",
            Some("Ubuntu"),
            home(HomeReason::OtherDistro),
        ),
        (
            r"\\wsl.localhost\Ubuntu\home",
            None,
            home(HomeReason::OtherDistro),
        ),
        // Network shares.
        (
            r"\\server\share\dir",
            Some("Ubuntu"),
            home(HomeReason::Network),
        ),
        (r"\\?\UNC\server\share", None, home(HomeReason::Network)),
        (
            r"\\wsl.localhost\",
            Some("Ubuntu"),
            home(HomeReason::Network),
        ),
        // Neither a drive nor a share.
        (r"Users\x", Some("Ubuntu"), home(HomeReason::NotAbsolute)),
        (r"C:relative", None, home(HomeReason::NotAbsolute)),
        ("", None, home(HomeReason::NotAbsolute)),
        ("/home/x", None, home(HomeReason::NotAbsolute)),
        (r"1:\x", None, home(HomeReason::NotAbsolute)),
    ];
    for (windows, distro, expected) in cases {
        assert_eq!(
            &windows_to_wsl_path(Path::new(windows), *distro),
            expected,
            "{windows} for {distro:?}"
        );
    }
}

#[test]
fn the_cd_argument_is_the_path_or_the_home() {
    assert_eq!(path("/mnt/c").as_arg(), "/mnt/c");
    assert_eq!(home(HomeReason::Network).as_arg(), "~");
    assert_eq!(path("/").notice_key(), None);
    assert_eq!(
        home(HomeReason::OtherDistro).notice_key(),
        Some("terminal.wsl.home_other_distro")
    );
}

/// The way back (RC-040-04): a folder a shell reports, as the panel's folder.
#[test]
fn reported_folders_translate_back_to_panel_paths() {
    let windows = ShellPaths::Native(Platform::Windows);
    let unix = ShellPaths::Native(Platform::Unix);
    let ubuntu = ShellPaths::Wsl(Some("Ubuntu".to_owned()));
    let unnamed = ShellPaths::Wsl(None);
    let cases: &[(&str, &ShellPaths, Option<&str>)] = &[
        // Windows: OSC 7's `/C:/...`, OSC 9;9's `C:\...`, forward slashes, the root.
        ("/C:/Users/x", &windows, Some(r"C:\Users\x")),
        ("/c:/Program Files/", &windows, Some(r"C:\Program Files")),
        (r"D:\a b\c", &windows, Some(r"D:\a b\c")),
        ("C:/x", &windows, Some(r"C:\x")),
        ("C:", &windows, Some(r"C:\")),
        (r"\\server\share\x\", &windows, Some(r"\\server\share\x")),
        ("C:relative", &windows, None),
        ("/home/x", &windows, None),
        ("relative", &windows, None),
        // Unix: absolute paths as they are.
        ("/home/x y", &unix, Some("/home/x y")),
        ("home/x", &unix, None),
        // WSL: drive mounts back to the drive, the rest into the distribution's share.
        ("/mnt/c/Users/x", &ubuntu, Some(r"C:\Users\x")),
        ("/mnt/d", &ubuntu, Some(r"D:\")),
        ("/home/x", &ubuntu, Some(r"\\wsl.localhost\Ubuntu\home\x")),
        ("/", &ubuntu, Some(r"\\wsl.localhost\Ubuntu")),
        (
            "/mnt/wsl/x",
            &ubuntu,
            Some(r"\\wsl.localhost\Ubuntu\mnt\wsl\x"),
        ),
        ("/mnt/c/x", &unnamed, Some(r"C:\x")),
        ("/home/x", &unnamed, None),
        ("C:\\x", &ubuntu, None),
    ];
    for (reported, paths, expected) in cases {
        assert_eq!(
            shell_to_panel_path(reported, paths),
            expected.map(std::path::PathBuf::from),
            "{reported} {paths:?}"
        );
    }
}

#[test]
fn panel_folders_become_the_shells_paths() {
    let ubuntu = ShellPaths::Wsl(Some("Ubuntu".to_owned()));
    assert_eq!(
        panel_to_shell_path(Path::new(r"C:\Users\x"), &ubuntu).as_deref(),
        Some("/mnt/c/Users/x")
    );
    assert_eq!(
        panel_to_shell_path(Path::new(r"\\wsl.localhost\Ubuntu\home\x"), &ubuntu).as_deref(),
        Some("/home/x")
    );
    assert_eq!(
        panel_to_shell_path(Path::new(r"\\server\share"), &ubuntu),
        None,
        "a network share has no path in the distribution"
    );
    assert_eq!(
        panel_to_shell_path(Path::new("/home/x"), &ShellPaths::Native(Platform::Unix)).as_deref(),
        Some("/home/x")
    );
}

#[test]
fn a_shell_fits_the_folders_of_its_own_system() {
    let native = ShellPaths::Native(Platform::Windows);
    let debian = ShellPaths::Wsl(Some("Debian".to_owned()));
    let unknown = ShellPaths::Wsl(None);
    // (folder, native, Debian's shell, a WSL shell of an unknown distribution)
    for (folder, mine, debians, unknowns) in [
        (r"C:\Users\x", true, false, false),
        (r"\\server\share\x", true, false, false),
        (r"\\wsl.localhost\Debian\home\x", false, true, true),
        (r"\\wsl$\debian", false, true, true),
        (r"\\?\UNC\wsl.localhost\DEBIAN\tmp", false, true, true),
        (r"\\wsl.localhost\Ubuntu\home", false, false, true),
    ] {
        let folder_path = Path::new(folder);
        assert_eq!(shell_fits(&native, folder_path), mine, "native {folder}");
        assert_eq!(shell_fits(&debian, folder_path), debians, "Debian {folder}");
        assert_eq!(shell_fits(&unknown, folder_path), unknowns, "WSL {folder}");
    }
}
