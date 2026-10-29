use std::path::{Path, PathBuf};

use super::{
    NetworkPath, WslPath, drive_letter, network_path, parse_wsl_unc, split_name, strip_verbatim,
    unverbatim, verbatim, wsl_unc_path,
};

fn verbatim_of(text: &str) -> Option<PathBuf> {
    verbatim(Path::new(text))
}

#[test]
fn a_drive_path_gets_the_prefix_that_lifts_the_limit() {
    assert_eq!(
        verbatim_of(r"D:\data\deep\file.txt"),
        Some(PathBuf::from(r"\\?\D:\data\deep\file.txt"))
    );
    assert_eq!(
        verbatim_of("D:/data/deep"),
        Some(PathBuf::from(r"\\?\D:\data\deep"))
    );
}

#[test]
fn a_share_and_a_wsl_distribution_go_under_unc() {
    assert_eq!(
        verbatim_of(r"\\nas\media\film"),
        Some(PathBuf::from(r"\\?\UNC\nas\media\film"))
    );
    assert_eq!(
        verbatim_of(r"\\wsl.localhost\Ubuntu\home\me"),
        Some(PathBuf::from(r"\\?\UNC\wsl.localhost\Ubuntu\home\me"))
    );
}

#[test]
fn what_cannot_be_made_verbatim_is_left_alone() {
    assert_eq!(verbatim_of(r"data\deep"), None);
    assert_eq!(verbatim_of(r"D:\data\..\deep"), None);
    assert_eq!(verbatim_of(r"\\nas"), None);
    assert_eq!(
        verbatim_of(r"\\?\D:\already"),
        Some(PathBuf::from(r"\\?\D:\already"))
    );
}

#[test]
fn long_path_changes_nothing_off_windows() {
    if !cfg!(windows) {
        assert_eq!(
            super::long_path(Path::new("/home/me")),
            PathBuf::from("/home/me")
        );
    }
}

#[test]
fn a_linux_path_becomes_a_wsl_share_path() {
    assert_eq!(
        wsl_unc_path("Ubuntu", "/home/me/src").expect("valid"),
        PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me\src")
    );
    assert_eq!(
        wsl_unc_path("Debian-12", "/").expect("valid"),
        PathBuf::from(r"\\wsl.localhost\Debian-12\")
    );
    assert_eq!(
        wsl_unc_path("Ubuntu", "//etc//").expect("valid"),
        PathBuf::from(r"\\wsl.localhost\Ubuntu\etc\\")
    );
}

#[test]
fn a_bad_distro_or_linux_path_is_refused() {
    for (distro, linux) in [
        ("", "/home"),
        ("Ub/untu", "/home"),
        ("Ub\\untu", "/home"),
        ("..", "/home"),
        ("Ubuntu", "home/me"),
        ("Ubuntu", "/home/a\\b"),
    ] {
        let error = wsl_unc_path(distro, linux).expect_err("invalid");
        assert_eq!(error.code(), "invalid_path", "{distro:?} {linux:?}");
    }
}

fn wsl(distro: &str, linux_path: &str) -> Option<WslPath> {
    Some(WslPath {
        distro: distro.to_owned(),
        linux_path: linux_path.to_owned(),
    })
}

#[test]
fn every_spelling_of_a_wsl_share_is_recognised() {
    for text in [
        r"\\wsl.localhost\Ubuntu\home\me",
        r"\\WSL.LOCALHOST\Ubuntu\home\me\",
        r"\\wsl$\Ubuntu\home\me",
        r"\\?\UNC\wsl.localhost\Ubuntu\home\me",
        "//wsl$/Ubuntu/home/me",
    ] {
        assert_eq!(
            parse_wsl_unc(Path::new(text)),
            wsl("Ubuntu", "/home/me"),
            "{text}"
        );
    }
    assert_eq!(
        parse_wsl_unc(Path::new(r"\\wsl$\Ubuntu")),
        wsl("Ubuntu", "/")
    );
}

#[test]
fn other_paths_are_not_wsl() {
    for text in [
        r"\\nas\Ubuntu\home",
        r"C:\wsl$\Ubuntu",
        "/home/me",
        r"\\wsl$\",
    ] {
        assert_eq!(parse_wsl_unc(Path::new(text)), None, "{text}");
    }
}

#[test]
fn building_and_parsing_agree() {
    let built = wsl_unc_path("Arch", "/var/log/pacman.log").expect("valid");
    assert_eq!(parse_wsl_unc(&built), wsl("Arch", "/var/log/pacman.log"));
}

#[test]
fn a_share_on_another_machine_is_a_network_path_in_every_spelling() {
    let share = |text: &str| Some(NetworkPath::Share(text.to_owned()));
    let cases: &[(&str, Option<NetworkPath>)] = &[
        (r"\\attacker\share", share(r"\\attacker\share")),
        (r"\\attacker\share\deep\x", share(r"\\attacker\share")),
        ("//attacker/share/x", share(r"\\attacker\share")),
        (r"/\\attacker\share", share(r"\\attacker\share")),
        (r"\\\attacker\share", share(r"\\attacker\share")),
        (r"\\?\UNC\attacker\share\x", share(r"\\attacker\share")),
        (r"\\.\unc\attacker\share", share(r"\\attacker\share")),
        (
            r"\\attacker@SSL@443\DavWWWRoot",
            share(r"\\attacker@SSL@443\DavWWWRoot"),
        ),
        (
            r"\\wsl.localhost.attacker.example\share",
            share(r"\\wsl.localhost.attacker.example\share"),
        ),
        (r"\\attacker", Some(NetworkPath::Other)),
        (r"\\attacker\", Some(NetworkPath::Other)),
        (r"\\wsl.localhost", Some(NetworkPath::Other)),
        (r"\\.\pipe\x", Some(NetworkPath::Other)),
        (
            r"\\?\GLOBALROOT\Device\Mup\attacker\share",
            Some(NetworkPath::Other),
        ),
        (r"\??\UNC\attacker\share", Some(NetworkPath::Other)),
    ];
    for (text, expected) in cases {
        assert_eq!(network_path(text), *expected, "{text}");
    }
}

#[test]
fn drives_wsl_shares_and_unix_paths_stay_on_this_machine() {
    for text in [
        r"C:\Users\x",
        "C:/Users/x",
        "/C:/Users/x",
        "C:",
        r"\\?\C:\long\path",
        r"\\.\D:\x",
        r"\Windows",
        "relative\\dir",
        "..",
        "/home/x",
        "",
        r"\\wsl.localhost\Ubuntu\home\x",
        r"\\WSL$\Ubuntu",
        "//wsl.localhost/Debian/etc",
        r"\\?\UNC\wsl.localhost\Ubuntu\home",
    ] {
        assert_eq!(network_path(text), None, "{text}");
    }
}

#[test]
fn a_drive_letter_is_a_letter_and_a_colon_whatever_follows() {
    assert_eq!(drive_letter("C:"), Some('C'));
    assert_eq!(drive_letter(r"C:\"), Some('C'));
    assert_eq!(drive_letter("c:/x"), Some('c'));
    assert_eq!(drive_letter("D:name"), Some('D'));
    for text in [
        "c",
        "",
        ":",
        "1:",
        "/C:",
        r"\\server\share",
        "C;x",
        "\u{e9}:",
    ] {
        assert_eq!(drive_letter(text), None, "{text:?}");
    }
}

#[test]
fn a_name_splits_at_its_last_dot_but_not_at_its_ends() {
    assert_eq!(split_name("notes.txt"), ("notes", "txt"));
    assert_eq!(split_name("a.tar.gz"), ("a.tar", "gz"));
    assert_eq!(split_name(".bashrc"), (".bashrc", ""));
    assert_eq!(split_name("notes."), ("notes.", ""));
    assert_eq!(split_name("notes"), ("notes", ""));
    assert_eq!(split_name("."), (".", ""));
    assert_eq!(split_name(""), ("", ""));
    assert_eq!(split_name("..x"), (".", "x"));
}

fn unverbatim_of(text: &str) -> Option<PathBuf> {
    unverbatim(Path::new(text))
}

#[test]
fn a_verbatim_drive_path_loses_the_prefix() {
    assert_eq!(
        unverbatim_of(r"\\?\C:\Users\x\link.txt"),
        Some(PathBuf::from(r"C:\Users\x\link.txt"))
    );
    assert_eq!(unverbatim_of(r"\\?\c:\"), Some(PathBuf::from(r"c:\")));
}

#[test]
fn a_verbatim_unc_path_becomes_the_share() {
    assert_eq!(
        unverbatim_of(r"\\?\UNC\server\share\dir"),
        Some(PathBuf::from(r"\\server\share\dir"))
    );
    assert_eq!(
        unverbatim_of(r"\\?\UNC\wsl.localhost\Ubuntu\home"),
        Some(PathBuf::from(r"\\wsl.localhost\Ubuntu\home"))
    );
}

#[test]
fn a_path_without_a_plain_form_stays() {
    for text in [
        r"C:\Users\x",
        r"\\server\share",
        r"\\.\C:\notdisk",
        r"\\?\GLOBALROOT\Device\ImDisk0\file.txt",
        r"\\?\serv\",
        r"\\?\C:foo",
        r"\\?\C:\foo\.\bar",
        r"\\?\UNC\server\share\..\other",
        "/home/x",
        "",
    ] {
        assert_eq!(unverbatim_of(text), None, "{text}");
    }
}

#[test]
fn strip_verbatim_only_changes_a_path_on_windows() {
    let verbatim = Path::new(r"\\?\C:\Users\x");
    let expected = if cfg!(windows) {
        PathBuf::from(r"C:\Users\x")
    } else {
        verbatim.to_path_buf()
    };
    assert_eq!(strip_verbatim(verbatim), expected);
    assert_eq!(
        strip_verbatim(Path::new("/home/x")),
        PathBuf::from("/home/x")
    );
}
