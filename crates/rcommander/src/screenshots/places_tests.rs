use rc_engine::{VfsPath, WslDistro};
use serde_json::json;

use super::{DEMO, Places, WSL, mentions, pick};

const WITH: Places<'static> = Places {
    demo: "C:\\Demo",
    wsl: Some("Ubuntu"),
};
const WITHOUT: Places<'static> = Places {
    demo: "C:\\Demo",
    wsl: None,
};

fn distro(name: &str, is_default: bool) -> WslDistro {
    WslDistro {
        name: name.to_owned(),
        is_default,
        version: Some(2),
        root: VfsPath::local(format!("\\\\wsl.localhost\\{name}\\")),
    }
}

#[test]
fn both_placeholders_are_replaced() {
    assert_eq!(
        WITH.fill("\\\\wsl.localhost\\{wsl}\\etc and {demo}\\Photos"),
        "\\\\wsl.localhost\\Ubuntu\\etc and C:\\Demo\\Photos"
    );
    assert_eq!(
        WITH.fill("{wsl}\tsolarized-light\tsolarized-dark"),
        "Ubuntu\tsolarized-light\tsolarized-dark"
    );
}

#[test]
fn a_text_without_a_placeholder_stays() {
    assert_eq!(WITH.fill("Projects"), "Projects");
    assert_eq!(WITHOUT.fill("Projects"), "Projects");
}

#[test]
fn without_a_distribution_the_wsl_placeholder_stays_and_demo_is_still_filled() {
    assert_eq!(
        WITHOUT.fill("{demo} and \\\\wsl.localhost\\{wsl}"),
        "C:\\Demo and \\\\wsl.localhost\\{wsl}"
    );
}

#[test]
fn every_string_of_a_value_is_filled_and_nothing_else() {
    let mut value = json!({
        "command": "settings.set",
        "params": { "key": "appearance.wsl_themes", "value": "{wsl}\tnord-light\tnord-dark" },
        "list": ["{demo}", 3, true, null],
        "{wsl}": "names are left alone"
    });
    WITH.place(&mut value);
    assert_eq!(
        value,
        json!({
            "command": "settings.set",
            "params": { "key": "appearance.wsl_themes", "value": "Ubuntu\tnord-light\tnord-dark" },
            "list": ["C:\\Demo", 3, true, null],
            "{wsl}": "names are left alone"
        })
    );
}

#[test]
fn a_placeholder_is_found_at_any_depth_in_values_only() {
    let deep = json!({ "command": { "params": { "path": ["x", "\\\\wsl.localhost\\{wsl}"] } } });
    assert!(mentions(&deep, WSL));
    assert!(!mentions(&deep, DEMO));
    assert!(!mentions(&json!({ "{wsl}": 1 }), WSL));
    assert!(!mentions(&json!({ "key": "Ctrl+F" }), WSL));
}

#[test]
fn the_default_distribution_is_picked_else_the_first() {
    let listed = [distro("docker-desktop", false), distro("Ubuntu", true)];
    assert_eq!(pick(&listed), Some("Ubuntu"));
    let none_default = [distro("Debian", false), distro("Ubuntu", false)];
    assert_eq!(pick(&none_default), Some("Debian"));
}

#[test]
fn no_distribution_picks_none() {
    assert_eq!(pick(&[]), None);
}
