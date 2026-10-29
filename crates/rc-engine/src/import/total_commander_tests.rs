use std::path::PathBuf;

use super::{
    TcConnection, TcMenuItem, TcMenuKind, TcSkip, TcTab, connections, expand, folder, hotlist, tabs,
};
use crate::{
    ConnectionKind, PanelId,
    import::{tc_ini, tc_password::TcPassword},
};

/// The fixtures' `%TC_TEST_HOME%`.
pub(crate) const HOME: &str = "C:\\Users\\me";

pub(crate) fn lookup(name: &str) -> Option<String> {
    (name == "TC_TEST_HOME").then(|| HOME.to_owned())
}

pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/total_commander")
        .join(name)
}

fn entry(
    index: usize,
    name: &str,
    folders: &[&str],
    path: &str,
    target: Option<&str>,
) -> TcMenuItem {
    TcMenuItem {
        index,
        name: name.to_owned(),
        folders: folders.iter().map(|folder| (*folder).to_owned()).collect(),
        kind: TcMenuKind::Entry {
            path: path.to_owned(),
            target: target.map(str::to_owned),
        },
    }
}

fn skipped(index: usize, name: &str, folders: &[&str], skip: TcSkip) -> TcMenuItem {
    TcMenuItem {
        index,
        name: name.to_owned(),
        folders: folders.iter().map(|folder| (*folder).to_owned()).collect(),
        kind: TcMenuKind::Skipped(skip),
    }
}

/// The directory menu both fixtures hold.
fn expected_menu() -> Vec<TcMenuItem> {
    vec![
        entry(1, "Windows", &[], "C:\\Windows", None),
        TcMenuItem {
            index: 2,
            name: "Projects".to_owned(),
            folders: Vec::new(),
            kind: TcMenuKind::Folder,
        },
        entry(
            3,
            "rCommander \u{e9}t\u{e9}",
            &["Projects"],
            "C:\\src\\rcommander",
            Some("D:\\build\\"),
        ),
        skipped(5, "Notepad", &["Projects"], TcSkip::Program),
        skipped(7, "Find files", &[], TcSkip::InternalCommand),
        skipped(8, "FTP server", &[], TcSkip::ServerPath),
        skipped(9, "Plugin", &[], TcSkip::PluginPath),
        skipped(10, "Relative", &[], TcSkip::RelativePath),
        skipped(11, "Unknown variable", &[], TcSkip::UnknownVariable),
        entry(12, "Home &more", &[], HOME, None),
    ]
}

fn tab(panel: PanelId, position: usize, path: Result<&str, TcSkip>) -> TcTab {
    TcTab {
        panel,
        position,
        path: path.map(str::to_owned),
        caption: None,
        locked: false,
        active: false,
    }
}

#[test]
fn the_menu_and_the_tabs_read_alike_from_utf16_and_ansi() {
    for name in ["wincmd_utf16.ini", "wincmd_ansi.ini"] {
        let ini = tc_ini::read_file(&fixture(name)).expect("fixture");
        assert_eq!(hotlist(&ini, &lookup), expected_menu(), "{name}");

        let left = tabs(&ini, PanelId::Left, &lookup);
        let expected = vec![
            TcTab {
                caption: Some("Win".to_owned()),
                ..tab(PanelId::Left, 0, Ok("C:\\Windows\\"))
            },
            TcTab {
                caption: Some("Work".to_owned()),
                active: true,
                ..tab(PanelId::Left, 1, Ok("C:\\Work\\"))
            },
            TcTab {
                locked: true,
                ..tab(PanelId::Left, 2, Ok("C:\\Users\\me\\Downloads\\"))
            },
            tab(PanelId::Left, 3, Ok("Z:\\gone\\")),
        ];
        assert_eq!(left, expected, "{name}");

        let right = tabs(&ini, PanelId::Right, &lookup);
        let expected = vec![
            tab(PanelId::Right, 0, Err(TcSkip::ServerPath)),
            TcTab {
                active: true,
                ..tab(PanelId::Right, 1, Ok("\\\\nas\\share\\"))
            },
        ];
        assert_eq!(right, expected, "{name}");
        assert_eq!(ini.get("FTP", "FtpIniName"), Some("wcx_ftp.ini"));
    }
}

#[test]
fn submenus_nest_and_a_skipped_folder_takes_its_contents_along() {
    let ini = tc_ini::parse(
        "[DirMenu]\nmenu1=-\nmenu2=-&\nmenu3=Inside\ncmd3=cd C:\\x\nmenu4=--\nmenu5=-A\nmenu6=-B\nmenu7=Deep\ncmd7=cd C:\\y\nmenu8=--\nmenu9=--\nmenu10=After\ncmd10=cd C:\n",
    );
    let items = hotlist(&ini, &lookup);
    assert_eq!(items[0].kind, TcMenuKind::Skipped(TcSkip::NameInvalid));
    assert_eq!(items[1].kind, TcMenuKind::Skipped(TcSkip::NameInvalid));
    assert_eq!(items[3].folders, vec!["A".to_owned()]);
    assert_eq!(items[4].folders, vec!["A".to_owned(), "B".to_owned()]);
    assert_eq!(items[5], entry(10, "After", &[], "C:\\", None));

    // Seven levels fit below the import's folder, an eighth does not.
    let mut text = String::from("[DirMenu]\n");
    for level in 1..=8 {
        text.push_str(&format!("menu{level}=-L{level}\n"));
    }
    let items = hotlist(&tc_ini::parse(&text), &lookup);
    assert_eq!(items[6].kind, TcMenuKind::Folder);
    assert_eq!(items[7].kind, TcMenuKind::Skipped(TcSkip::TooDeep));
}

#[test]
fn folders_are_absolute_and_variables_known() {
    assert_eq!(
        folder("\"C:\\Program Files\"", &lookup),
        Ok("C:\\Program Files".to_owned())
    );
    assert_eq!(folder("d:", &lookup), Ok("d:\\".to_owned()));
    assert_eq!(folder("/home/me", &lookup), Ok("/home/me".to_owned()));
    assert_eq!(
        folder("\\\\wsl.localhost\\Ubuntu", &lookup),
        Ok("\\\\wsl.localhost\\Ubuntu".to_owned())
    );
    assert_eq!(folder("", &lookup), Err(TcSkip::InvalidPath));
    assert_eq!(folder("C:\\a\u{7}b", &lookup), Err(TcSkip::InvalidPath));
    assert_eq!(
        expand("%TC_TEST_HOME%\\x", &lookup),
        Some(format!("{HOME}\\x"))
    );
    assert_eq!(expand("100%", &lookup), None);
    assert_eq!(expand("%a b%", &lookup), None);
}

fn connection(section: &str, kind: Result<ConnectionKind, TcSkip>, host: &str) -> TcConnection {
    let mut parts: Vec<String> = section.split('|').map(str::to_owned).collect();
    let name = parts.pop().unwrap_or_default();
    TcConnection {
        section: section.to_owned(),
        name,
        folders: parts,
        kind,
        host: host.to_owned(),
        port: None,
        user: None,
        directory: None,
        password: TcPassword::None,
    }
}

#[test]
fn the_connections_of_wcx_ftp_ini() {
    let ini = tc_ini::read_file(&fixture("wcx_ftp.ini")).expect("fixture");
    let found = connections(&ini);
    let expected = vec![
        TcConnection {
            user: Some("alice".to_owned()),
            directory: Some("/pub".to_owned()),
            password: TcPassword::Present,
            ..connection("Plain FTP", Ok(ConnectionKind::Ftp), "ftp.example.com")
        },
        TcConnection {
            port: Some(2121),
            user: Some("bob".to_owned()),
            password: TcPassword::Present,
            ..connection(
                "Work|Secure FTPS",
                Ok(ConnectionKind::Ftps),
                "secure.example.com",
            )
        },
        TcConnection {
            port: Some(2222),
            user: Some("carol".to_owned()),
            ..connection(
                "Work|Servers|SFTP box",
                Ok(ConnectionKind::Ssh),
                "2001:db8::1",
            )
        },
        TcConnection {
            user: Some("dave".to_owned()),
            password: TcPassword::MasterPassword,
            ..connection("Locked", Ok(ConnectionKind::Ftp), "locked.example.com")
        },
        connection("Anonymous", Ok(ConnectionKind::Ftp), "mirror.example.com"),
        TcConnection {
            port: Some(990),
            user: Some("erin".to_owned()),
            ..connection(
                "Implicit",
                Ok(ConnectionKind::FtpsImplicit),
                "implicit.example.com",
            )
        },
        connection("Gopher", Err(TcSkip::ProtocolUnknown), "old.example.com"),
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_bad_port_is_reported() {
    let ini = tc_ini::parse("[connections]\n1=x\n[x]\nhost=example.com:99999\n");
    assert_eq!(connections(&ini)[0].kind, Err(TcSkip::PortInvalid));
}

#[test]
fn every_skip_has_its_own_code() {
    let mut codes: Vec<&str> = TcSkip::ALL.iter().map(|skip| skip.code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), TcSkip::ALL.len());
}

/// Total Commander keeps a connection's folders with `|` (`DEGOYA|wedding`); a backslash is part
/// of the name (Windows test 2026-10-18: imported names read `GROUP|name`).
#[test]
fn a_pipe_puts_a_connection_into_folders_and_a_backslash_stays_in_its_name() {
    let ini = tc_ini::parse(
        "[connections]\n1=DEGOYA|Web|wedding\n2=a\\b\n[DEGOYA|Web|wedding]\nhost=w.example\n[a\\b]\nhost=b.example\n",
    );
    let found = connections(&ini);
    assert_eq!(found[0].folders, ["DEGOYA", "Web"]);
    assert_eq!(found[0].name, "wedding");
    assert!(found[1].folders.is_empty());
    assert_eq!(found[1].name, "a\\b");
}
