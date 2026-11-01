use rc_engine::{DriveGroup, DriveState};

use super::*;
use crate::app::workspaces::fixtures::panel;

fn drive(
    id: &str,
    group: DriveGroup,
    kind: DriveKind,
    label: &str,
    root: Option<VfsPath>,
) -> Drive {
    Drive {
        id: id.to_owned(),
        group,
        kind,
        label: label.to_owned(),
        root,
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        folder_buttons: Vec::new(),
        favourite: false,
        remote: None,
    }
}

fn sftp(connection_id: i64) -> VfsPath {
    VfsPath {
        backend: BackendId::Sftp { connection_id },
        path: PathBuf::from("/"),
    }
}

/// A drive, a distribution, an SSH server and an FTP server without a root yet.
fn drives() -> Vec<Drive> {
    vec![
        drive(
            "C:",
            DriveGroup::Drives,
            DriveKind::Fixed,
            "C:",
            Some(VfsPath::local("C:\\")),
        ),
        drive(
            "\\\\wsl.localhost\\Ubuntu",
            DriveGroup::Wsl,
            DriveKind::Wsl,
            "Ubuntu",
            Some(VfsPath::local("\\\\wsl.localhost\\Ubuntu\\")),
        ),
        drive(
            "server:4",
            DriveGroup::Servers,
            DriveKind::Server,
            "build",
            Some(sftp(4)),
        ),
        drive(
            "server:6",
            DriveGroup::Servers,
            DriveKind::Server,
            "files",
            None,
        ),
    ]
}

/// The places are the drive bar's entries with their roots; an FTP server's is its `/`.
#[test]
fn the_places_are_the_drive_bars() {
    let places = places(&drives());
    let labels: Vec<&str> = places.iter().map(|place| place.label.as_str()).collect();
    assert_eq!(labels, ["C:", "Ubuntu", "build", "files"]);
    assert_eq!(places[2].root, sftp(4));
    assert_eq!(
        places[3].root,
        VfsPath {
            backend: BackendId::Ftp { connection_id: 6 },
            path: PathBuf::from("/"),
        }
    );
    assert_eq!(distros(&drives()), ["Ubuntu"]);
    assert_eq!(
        ssh_servers(&drives()),
        [(ConnectionId(4), "build".to_owned())]
    );
}

/// Up and Down move a tab and the active tab with it; neither goes past an end.
#[test]
fn tabs_move_with_the_active_one() {
    let mut spec = panel(&["/a", "/b", "/c"]);
    apply(&mut spec, PanelMsg::Down(0));
    let paths: Vec<PathBuf> = spec
        .tabs
        .iter()
        .map(|tab| tab.location.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            PathBuf::from("/b"),
            PathBuf::from("/a"),
            PathBuf::from("/c")
        ]
    );
    assert_eq!(spec.active_tab, 1, "the active tab went along");
    apply(&mut spec, PanelMsg::Up(0));
    apply(&mut spec, PanelMsg::Down(2));
    assert_eq!(
        spec.tabs[0].location.path,
        PathBuf::from("/b"),
        "nothing above the first"
    );
    assert_eq!(
        spec.tabs[2].location.path,
        PathBuf::from("/c"),
        "nothing below the last"
    );
    apply(&mut spec, PanelMsg::Up(2));
    assert_eq!(
        spec.active_tab, 2,
        "the active tab moved down by the other one's move"
    );
}

/// A removed tab takes the active one to the tab before; the last tab stays.
#[test]
fn the_last_tab_stays() {
    let mut spec = panel(&["/a", "/b", "/c"]);
    spec.active_tab = 2;
    apply(&mut spec, PanelMsg::Remove(2));
    assert_eq!(spec.tabs.len(), 2);
    assert_eq!(spec.active_tab, 1);
    apply(&mut spec, PanelMsg::Remove(0));
    assert_eq!(spec.active_tab, 0, "still /b");
    assert_eq!(spec.tabs[0].location.path, PathBuf::from("/b"));
    apply(&mut spec, PanelMsg::Remove(0));
    assert_eq!(spec.tabs.len(), 1, "a panel keeps a tab");
    apply(&mut spec, PanelMsg::ActiveTab(5));
    assert_eq!(spec.active_tab, 0, "no tab 5");
}

/// A place sets the root, a typed path the path on the same place; a new tab is unlocked at the
/// folder given; a lock and the active tab are kept per tab.
#[test]
fn a_tab_is_edited_in_place() {
    let mut spec = panel(&["/a"]);
    apply(&mut spec, PanelMsg::Place(0, sftp(4)));
    apply(&mut spec, PanelMsg::Path(0, "/srv/www".to_owned()));
    assert_eq!(
        spec.tabs[0].location,
        VfsPath {
            backend: BackendId::Sftp { connection_id: 4 },
            path: PathBuf::from("/srv/www"),
        }
    );
    apply(&mut spec, PanelMsg::Locked(0, true));
    apply(&mut spec, PanelMsg::AddTab(VfsPath::local("/home")));
    assert_eq!(spec.tabs.len(), 2);
    assert!(spec.tabs[0].locked);
    assert!(!spec.tabs[1].locked, "a new tab is not locked");
    assert_eq!(spec.tabs[1].location, VfsPath::local("/home"));
    apply(&mut spec, PanelMsg::ActiveTab(1));
    assert_eq!(spec.active_tab, 1);
}

/// A terminal view starts a local shell until another is chosen; the files view keeps the
/// terminal for the next switch.
#[test]
fn a_terminal_view_has_a_terminal() {
    let mut spec = panel(&["/a"]);
    apply(&mut spec, PanelMsg::View(PanelView::Terminal));
    assert_eq!(
        spec.terminal,
        Some(TermSpec::Local {
            shell: None,
            cwd: None,
        })
    );
    let ssh = TermSpec::Ssh {
        connection_id: ConnectionId(4),
        cwd: Some("/srv".to_owned()),
    };
    apply(&mut spec, PanelMsg::Terminal(ssh.clone()));
    apply(&mut spec, PanelMsg::View(PanelView::Files));
    apply(&mut spec, PanelMsg::View(PanelView::Terminal));
    assert_eq!(spec.terminal, Some(ssh));
}

/// Switching the kind keeps the start folder and needs a distribution or a server; an empty
/// folder or shell is the default.
#[test]
fn a_terminal_switches_its_kind() {
    let local = TermSpec::Local {
        shell: Some("pwsh".to_owned()),
        cwd: Some("C:\\work".to_owned()),
    };
    assert_eq!(kind_of(&local), TermKind::Local);
    assert_eq!(
        switched(Some(&local), TermKind::Wsl, Some("Ubuntu"), None),
        Some(TermSpec::Wsl {
            distro: "Ubuntu".to_owned(),
            cwd: Some("C:\\work".to_owned()),
        })
    );
    assert_eq!(switched(Some(&local), TermKind::Wsl, None, None), None);
    assert_eq!(switched(None, TermKind::Ssh, None, None), None);
    assert_eq!(
        switched(None, TermKind::Ssh, None, Some(ConnectionId(4))),
        Some(TermSpec::Ssh {
            connection_id: ConnectionId(4),
            cwd: None,
        })
    );
    let cleared = with_cwd(&local, "  ".to_owned());
    assert_eq!(cwd_of(&cleared), "");
    assert_eq!(
        with_shell(&cleared, String::new()),
        TermSpec::Local {
            shell: None,
            cwd: None,
        }
    );
    let wsl = TermSpec::Wsl {
        distro: "Ubuntu".to_owned(),
        cwd: None,
    };
    assert_eq!(
        with_shell(&wsl, "bash".to_owned()),
        wsl,
        "a shell is a local one's"
    );
}
