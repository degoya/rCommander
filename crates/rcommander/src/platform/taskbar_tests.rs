//! The values a window gives the taskbar: the ID per installation, the quoted relaunch command.

use std::path::Path;

use super::{APP_ID_PREFIX, Relaunch, app_id, fnv1a, plain, relaunch_of};

const PROGRAM: &str = r"C:\Users\Owner\Tools\rCommander\rcommander.exe";

#[test]
fn the_hash_is_fnv1a_with_its_published_values() {
    assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn the_relaunch_is_the_quoted_program_and_an_id_of_its_path() {
    let relaunch = relaunch_of(Path::new(PROGRAM)).expect("a plain Windows path");
    assert_eq!(
        relaunch,
        Relaunch {
            app_id: "degoya.rCommander.0520a250a3035586".to_owned(),
            command: format!("\"{PROGRAM}\""),
        }
    );
}

#[test]
fn a_path_with_spaces_is_one_argument_and_the_id_has_no_space() {
    let program = r"C:\Program Files\rCommander 0.15\rcommander.exe";
    let relaunch = relaunch_of(Path::new(program)).expect("a plain Windows path");
    assert_eq!(relaunch.command, format!("\"{program}\""));
    assert!(!relaunch.app_id.contains(' '));
    assert!(relaunch.app_id.len() <= 128);
    assert!(relaunch.app_id.starts_with(APP_ID_PREFIX));
}

#[test]
fn the_id_ignores_case_as_windows_paths_do() {
    assert_eq!(app_id(PROGRAM), app_id(&PROGRAM.to_uppercase()));
}

#[test]
fn each_installation_has_its_own_id() {
    let development = r"C:\Users\Owner\AppData\Local\rcommander-dev\rcommander.exe";
    assert_ne!(app_id(PROGRAM), app_id(development));
}

#[test]
fn the_verbatim_prefix_is_dropped() {
    assert_eq!(plain(&format!(r"\\?\{PROGRAM}")), PROGRAM);
    assert_eq!(
        plain(r"\\?\UNC\server\share\rcommander.exe"),
        r"\\server\share\rcommander.exe"
    );
    // Not a drive after the prefix: left as it is.
    assert_eq!(
        plain(r"\\?\Volume{1234}\rcommander.exe"),
        r"\\?\Volume{1234}\rcommander.exe"
    );
    let verbatim = relaunch_of(Path::new(&format!(r"\\?\{PROGRAM}"))).expect("a verbatim path");
    assert_eq!(Some(verbatim), relaunch_of(Path::new(PROGRAM)));
}

#[test]
fn a_path_no_command_line_can_carry_gets_no_relaunch() {
    assert_eq!(relaunch_of(Path::new("")), None);
    assert_eq!(
        relaunch_of(Path::new("C:\\odd\"name\\rcommander.exe")),
        None
    );
}
