use std::path::PathBuf;

use rc_db::connections::{ConnectionId, ConnectionThemes};
use rc_engine::settings::{
    parse_wsl_themes,
    themes::{BUILTIN_THEMES, Theme, ThemesState},
};
use rc_vfs::{BackendId, VfsPath};

use super::{Assigned, connection_of, distro_of, of_connection, of_distro, of_path, set_assigned};
use crate::theme::{
    terminal_palette,
    themes::{set_state, tokens_of},
    tokens,
};

fn row(id: i64, light: Option<&str>, dark: Option<&str>) -> ConnectionThemes {
    ConnectionThemes {
        id: ConnectionId(id),
        light: light.map(str::to_owned),
        dark: dark.map(str::to_owned),
    }
}

/// Every built-in theme this system offers, Fluent chosen for both modes.
fn builtins() -> ThemesState {
    ThemesState {
        themes: BUILTIN_THEMES
            .iter()
            .filter(|theme| theme.available())
            .map(Theme::of_builtin)
            .collect(),
        chosen: ["fluent-light".into(), "fluent-dark".into()],
    }
}

fn server(connection_id: i64) -> VfsPath {
    VfsPath {
        backend: BackendId::Sftp { connection_id },
        path: PathBuf::from("/srv"),
    }
}

#[test]
fn a_connection_names_a_theme_per_mode_while_the_switch_is_on() {
    let assigned = Assigned::of(true, vec![row(7, Some("nord-light"), None)]);
    assert_eq!(assigned.theme(7, false), Some("nord-light"));
    assert_eq!(assigned.theme(7, true), None, "none for the dark mode");
    assert_eq!(assigned.theme(8, false), None, "another connection");
    let off = Assigned::of(false, vec![row(7, Some("nord-light"), None)]);
    assert_eq!(off.theme(7, false), None, "the switch off");
    assert_eq!(off.themes.len(), 1, "the switch keeps the assignment");
}

#[test]
fn only_a_server_location_is_a_connection_s() {
    assert_eq!(connection_of(&server(3)), Some(3));
    let ftp = VfsPath {
        backend: BackendId::Ftp { connection_id: 4 },
        path: PathBuf::from("/"),
    };
    assert_eq!(connection_of(&ftp), Some(4));
    assert_eq!(connection_of(&VfsPath::local("/home")), None);
}

#[test]
fn a_panel_and_a_terminal_take_their_connection_s_look_per_mode() {
    set_state(&builtins());
    set_assigned(Assigned::of(
        true,
        vec![
            row(1, Some("nord-light"), Some("solarized-dark")),
            // A theme of the wrong mode, and one the window does not know (deleted).
            row(2, Some("nord-dark"), Some("own-gone")),
        ],
    ));
    let nord = tokens_of("nord-light", false).expect("nord");
    let solarized = tokens_of("solarized-dark", true).expect("solarized");
    assert!(std::ptr::eq(of_path(&server(1), false), nord));
    assert!(std::ptr::eq(of_connection(Some(1), true), solarized));
    assert_eq!(
        terminal_palette(of_connection(Some(1), true)).background,
        terminal_palette(solarized).background,
        "an SSH terminal draws with the connection's palette"
    );
    for dark in [false, true] {
        let window = tokens(dark);
        assert!(
            std::ptr::eq(of_connection(Some(2), dark), window),
            "falls back"
        );
        assert!(
            std::ptr::eq(of_connection(Some(9), dark), window),
            "no theme"
        );
        assert!(
            std::ptr::eq(of_connection(None, dark), window),
            "this machine"
        );
        assert!(std::ptr::eq(of_path(&VfsPath::local("/"), dark), window));
    }

    set_assigned(Assigned::of(
        false,
        vec![row(1, Some("nord-light"), Some("solarized-dark"))],
    ));
    assert!(
        std::ptr::eq(of_path(&server(1), false), tokens(false)),
        "the switch off draws every panel in the window's theme"
    );
    set_assigned(Assigned::default());
}

fn share(text: &str) -> VfsPath {
    VfsPath::local(text)
}

#[test]
fn a_wsl_share_is_its_distribution_s_in_either_form() {
    for path in [
        r"\\wsl.localhost\Ubuntu\home\me",
        r"\\wsl$\Ubuntu\home",
        r"\\?\UNC\wsl.localhost\Ubuntu",
        "//wsl.localhost/Ubuntu/etc",
    ] {
        assert_eq!(distro_of(&share(path)).as_deref(), Some("Ubuntu"), "{path}");
    }
    assert_eq!(distro_of(&share(r"\\server\share\x")), None);
    assert_eq!(distro_of(&share("/home/me")), None);
    assert_eq!(distro_of(&server(1)), None, "a server is no distribution");
}

/// RC-100-56: a panel on a distribution's share and its WSL terminal take its theme per mode;
/// another distribution, a deleted theme, one of the other mode and the switch off draw with the
/// window's.
#[test]
fn a_distribution_s_panel_and_terminal_take_its_look_per_mode() {
    set_state(&builtins());
    // Debian names a dark theme for the light mode, Kali one the window does not know (deleted).
    let distros = parse_wsl_themes(
        "Ubuntu\tnord-light\tsolarized-dark\nDebian\tnord-dark\nKali\town-gone\town-gone",
    )
    .expect("valid");
    set_assigned(Assigned::of(true, Vec::new()).with_distros(distros.clone()));
    let nord = tokens_of("nord-light", false).expect("nord");
    let solarized = tokens_of("solarized-dark", true).expect("solarized");
    for form in [r"\\wsl.localhost\Ubuntu\home", r"\\wsl$\ubuntu\home"] {
        assert!(std::ptr::eq(of_path(&share(form), false), nord), "{form}");
        assert!(
            std::ptr::eq(of_path(&share(form), true), solarized),
            "{form}"
        );
    }
    assert!(std::ptr::eq(of_distro(Some("UBUNTU"), true), solarized));
    assert_eq!(
        terminal_palette(of_distro(Some("Ubuntu"), true)).background,
        terminal_palette(solarized).background,
        "a WSL terminal draws with its distribution's palette"
    );
    for dark in [false, true] {
        let window = tokens(dark);
        for distro in ["Debian", "Kali", "Arch"] {
            assert!(
                std::ptr::eq(of_distro(Some(distro), dark), window),
                "{distro}: the other mode, a deleted theme, or none"
            );
        }
        assert!(std::ptr::eq(of_distro(None, dark), window), "a local shell");
        assert!(std::ptr::eq(of_path(&share(r"C:\Users"), dark), window));
    }

    set_assigned(Assigned::of(false, Vec::new()).with_distros(distros));
    for dark in [false, true] {
        assert!(
            std::ptr::eq(
                of_path(&share(r"\\wsl.localhost\Ubuntu"), dark),
                tokens(dark)
            ),
            "the switch off draws every panel in the window's theme"
        );
    }
    set_assigned(Assigned::default());
}
