//! A workspace's places (RC-110-21): which folders this machine opens, and a panel in words.

use rc_vfs::{BackendId, VfsPath};

use super::super::{TabSpec, TermSpec, spec::sample};
use crate::PanelView;

fn tab(location: VfsPath) -> TabSpec {
    TabSpec {
        location,
        sort: None,
        mode: None,
        locked: false,
        title: None,
    }
}

#[test]
fn the_local_places_are_the_tabs_of_this_machine_and_every_start_folder() {
    let mut spec = sample();
    spec.left.tabs = vec![
        tab(VfsPath::local("//attacker/share/x")),
        tab(VfsPath {
            backend: BackendId::Sftp { connection_id: 4 },
            path: "//not/judged".into(),
        }),
    ];
    spec.left.active_tab = 0;
    // A start folder counts whatever the panel shows: the spec keeps it.
    spec.right.terminal = Some(TermSpec::Wsl {
        distro: "Ubuntu".into(),
        cwd: Some(r"\\attacker\share".into()),
    });
    let places = spec.local_places();
    assert!(
        places.contains(&"//attacker/share/x".to_owned()),
        "{places:?}"
    );
    assert!(
        places.contains(&r"\\attacker\share".to_owned()),
        "{places:?}"
    );
    assert!(!places.contains(&"//not/judged".to_owned()), "{places:?}");
    // The sample's one tab of the right panel, and the two above.
    assert_eq!(places.len(), 3, "{places:?}");
}

#[test]
fn a_panel_is_shown_by_its_tabs_and_the_terminal_it_would_show() {
    let mut spec = sample();
    spec.left.tabs = vec![
        tab(VfsPath::local("/srv/www")),
        tab(VfsPath {
            backend: BackendId::Sftp { connection_id: 4 },
            path: "/var/www".into(),
        }),
    ];
    spec.left.active_tab = 0;
    assert_eq!(spec.left.shown_tabs(), ["/srv/www", "sftp://#4/var/www"]);
    spec.left.terminal = Some(TermSpec::Local {
        shell: Some("pwsh".into()),
        cwd: Some("/srv".into()),
    });
    assert_eq!(spec.left.shown_terminal(), None, "the file list is shown");
    spec.left.view = PanelView::Terminal;
    assert_eq!(
        spec.left.shown_terminal().as_deref(),
        Some("local:pwsh in /srv")
    );
    spec.left.terminal = Some(TermSpec::Wsl {
        distro: "Ubuntu".into(),
        cwd: None,
    });
    assert_eq!(spec.left.shown_terminal().as_deref(), Some("wsl:Ubuntu"));
}
