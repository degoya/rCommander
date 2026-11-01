use std::time::Duration;

use rc_engine::{Command, CommandClass, PanelId};
use serde_json::{Value, json};

use super::{Capture, Look, MAX_SCENES, MAX_STEPS, NO_WSL, Opening, Step, check_name, parse};

const DEMO: &str = "C:\\Demo";
/// The WSL distribution `{wsl}` stands for; `None` for a machine without one.
const WSL: Option<&str> = Some("Ubuntu");

/// The repository's own scenes and their alt texts (`scripts/screenshots/`).
const SCENES: &str = include_str!("../../../../scripts/screenshots/scenes.json");
const ALTS: &str = include_str!("../../../../scripts/screenshots/alts.json");

fn file(scenes: Value) -> String {
    json!({ "version": 1, "scenes": scenes }).to_string()
}

fn one(steps: Value) -> String {
    file(json!([{ "name": "shot", "steps": steps }]))
}

fn refused(text: &str, says: &str) {
    let error = parse(text, DEMO, WSL).expect_err("refused");
    let message = format!("{error:#}");
    assert!(message.contains(says), "{message:?} does not say {says:?}");
}

#[test]
fn the_repository_scenes_parse_and_each_has_an_alt_text() {
    let plan = parse(SCENES, DEMO, WSL).expect("scripts/screenshots/scenes.json");
    let alts: serde_json::Map<String, Value> = serde_json::from_str(ALTS).expect("alts.json");
    let names: Vec<&str> = plan
        .scenes
        .iter()
        .map(|scene| scene.name.as_str())
        .collect();
    let mut described: Vec<&str> = alts.keys().map(String::as_str).collect();
    described.sort_unstable();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, described, "one alt text per scene, no other");
    assert!(
        alts.values()
            .all(|alt| alt.as_str().is_some_and(|alt| !alt.is_empty()))
    );
    for wanted in [
        "panels-dark",
        "panels-light",
        "terminal",
        "connection-manager",
        "multi-rename",
        "lister",
        "compare",
        "synchronize",
        "settings",
        "settings-themes",
        "mcp-question",
        "archive-in-panel",
        "wsl-theme",
    ] {
        assert!(names.contains(&wanted), "the scene {wanted} is missing");
    }
}

/// The run turns `appearance.dialog_windows` off (`prepare`), so a Lister lies over the main
/// window and never has a window of its own to capture or close (Windows run 2026-10-29).
#[test]
fn the_repository_scenes_never_wait_for_a_lister_window() {
    let plan = parse(SCENES, DEMO, WSL).expect("scripts/screenshots/scenes.json");
    for scene in &plan.scenes {
        assert_ne!(scene.capture, Capture::Lister, "scene {}", scene.name);
        assert!(
            !scene
                .steps
                .iter()
                .any(|step| matches!(step, Step::Close(Capture::Lister))),
            "scene {} closes a Lister window",
            scene.name
        );
    }
}

#[test]
fn every_step_kind_parses_with_its_defaults() {
    let text = one(json!([
        { "key": "Ctrl+F" },
        { "command": { "command": "panel.navigate", "params": { "panel": "left", "path": "{demo}\\Photos" } } },
        { "wait_ms": 300 },
        { "open": "settings" },
        { "close": "lister" },
        { "mcp_question": {
            "tool": "ops_copy",
            "class": "write",
            "arguments": [{ "name": "target", "value": "{demo}\\Downloads" }],
            "session": true
        } }
    ]));
    let plan = parse(&text, DEMO, WSL).expect("plan");
    assert_eq!((plan.width, plan.height), (1280, 800));
    assert_eq!(plan.startup, Duration::from_millis(2500));
    let scene = &plan.scenes[0];
    assert_eq!(scene.theme, None);
    assert_eq!(scene.capture, Capture::Main);
    assert_eq!(scene.settle, Duration::from_millis(800));
    let [key, command, wait, open, close, ask] = scene.steps.as_slice() else {
        panic!("six steps: {:?}", scene.steps);
    };
    assert!(matches!(key, Step::Key(chord) if chord.to_string() == "Ctrl+F"));
    assert!(matches!(
        command,
        Step::Command(command) if **command == Command::Navigate {
            panel: Some(PanelId::Left),
            path: "C:\\Demo\\Photos".to_owned(),
        }
    ));
    assert!(matches!(wait, Step::Wait(wait) if *wait == Duration::from_millis(300)));
    assert!(matches!(
        open,
        Step::Open(Opening::Settings {
            page: None,
            tab: None
        })
    ));
    assert!(matches!(close, Step::Close(Capture::Lister)));
    let Step::Ask(ask) = ask else {
        panic!("a question: {ask:?}");
    };
    assert_eq!(ask.tool, "ops_copy");
    assert_eq!(ask.class, CommandClass::Write);
    assert_eq!(ask.arguments[0].value, "C:\\Demo\\Downloads");
    assert!(ask.session);
}

/// RC-100-55: an `open` step names the settings' page and tab; they stand beside `open` only, and
/// one the settings do not have ends the run.
#[test]
fn the_settings_open_on_a_page_and_a_tab() {
    let text = one(json!([{ "open": "settings", "page": "appearance", "tab": "themes" }]));
    let plan = parse(&text, DEMO, WSL).expect("plan");
    assert!(matches!(
        plan.scenes[0].steps.as_slice(),
        [Step::Open(Opening::Settings {
            page: Some(crate::app::settings::Page::Appearance),
            tab: Some(crate::app::settings::Tab::Themes),
        })]
    ));
    refused(
        &one(json!([{ "open": "settings", "page": "colours" }])),
        "no page 'colours'",
    );
    refused(
        &one(json!([{ "open": "settings", "page": "appearance", "tab": "palette" }])),
        "no tab 'palette'",
    );
    refused(
        &one(json!([{ "open": "settings", "page": "appearance", "colour": "red" }])),
        "does not parse",
    );
    refused(
        &one(json!([{ "key": "F5", "page": "appearance" }])),
        "does not parse",
    );
}

#[test]
fn the_file_level_values_are_read() {
    let text = json!({
        "version": 1,
        "size": { "width": 1600, "height": 1000 },
        "startup_ms": 5000,
        "scenes": [{ "name": "a", "theme": "light", "capture": "settings", "settle_ms": 0 }]
    })
    .to_string();
    let plan = parse(&text, DEMO, WSL).expect("plan");
    assert_eq!((plan.width, plan.height), (1600, 1000));
    assert_eq!(plan.startup, Duration::from_millis(5000));
    assert_eq!(plan.scenes[0].theme, Some(Look::Light));
    assert_eq!(plan.scenes[0].capture, Capture::Settings);
    assert_eq!(plan.scenes[0].settle, Duration::ZERO);
}

#[test]
fn an_unknown_field_anywhere_is_refused() {
    refused(
        &json!({ "version": 1, "scenes": [{ "name": "a" }], "extra": 1 }).to_string(),
        "unknown field",
    );
    refused(&file(json!([{ "name": "a", "zoom": 2 }])), "unknown field");
    refused(
        &json!({ "version": 1, "size": { "width": 800, "height": 600, "dpi": 2 }, "scenes": [{ "name": "a" }] }).to_string(),
        "unknown field",
    );
    refused(&one(json!([{ "type": "hello" }])), "unknown variant");
    refused(
        &one(json!([{ "key": "F5", "wait_ms": 1 }])),
        "does not parse",
    );
    refused(
        &one(json!([{ "mcp_question": { "tool": "a", "class": "read", "colour": "red" } }])),
        "unknown field",
    );
    refused(
        &one(
            json!([{ "mcp_question": { "tool": "a", "class": "read", "arguments": [{ "name": "x", "value": "y", "z": 1 }] } }]),
        ),
        "unknown field",
    );
}

#[test]
fn a_command_with_an_unknown_parameter_or_field_is_refused() {
    refused(
        &one(
            json!([{ "command": { "command": "panel.navigate", "params": { "path": "x", "speed": 3 } } }]),
        ),
        // Refused by serde or by the check behind it, by name either way.
        "speed",
    );
    refused(
        &one(json!([{ "command": { "command": "panel.refresh", "params": {}, "when": "now" } }])),
        "has no field 'when'",
    );
    refused(
        &one(json!([{ "command": { "command": "panel.no_such_thing", "params": {} } }])),
        "does not parse",
    );
    refused(
        &one(json!([{ "command": "panel.refresh" }])),
        "is an object",
    );
}

#[test]
fn only_reading_and_writing_commands_run() {
    refused(
        &one(json!([{ "command": { "command": "diagnostics.clear_error_log" } }])),
        "is of class destructive",
    );
    refused(
        &one(json!([{ "command": { "command": "panel.toggle_terminal", "params": {} } }])),
        "is of class exec",
    );
    refused(
        &one(json!([{ "command": { "command": "app.quit", "params": {} } }])),
        "is of class gui_only",
    );
}

#[test]
fn a_bad_key_a_bad_tool_and_closing_the_main_window_are_refused() {
    refused(&one(json!([{ "key": "Hyper+X" }])), "the key 'Hyper+X'");
    refused(
        &one(json!([{ "mcp_question": { "tool": "Ops Copy", "class": "write" } }])),
        "the tool 'Ops Copy'",
    );
    refused(&one(json!([{ "close": "main" }])), "main window");
}

#[test]
fn scene_names_stay_one_plain_file() {
    for good in ["a", "panels-dark", "0-intro", "x".repeat(64).as_str()] {
        check_name(good).unwrap_or_else(|error| panic!("{good}: {error}"));
    }
    let long = "x".repeat(65);
    for bad in [
        "",
        "-lead",
        "Upper",
        "a.b",
        "a/b",
        "a\\b",
        "../up",
        "..",
        "C:x",
        "a b",
        "umlaut-\u{e4}",
        long.as_str(),
    ] {
        assert!(check_name(bad).is_err(), "{bad:?} passed");
    }
    for device in ["con", "prn", "aux", "nul", "com1", "lpt9"] {
        let error = check_name(device).expect_err("a device name");
        assert!(
            error.to_string().contains("device name"),
            "{device}: {error}"
        );
    }
    check_name("com0").expect("com0 is no device");
    check_name("console").expect("console is no device");
    refused(
        &file(json!([{ "name": "../evil" }])),
        "the scene name '../evil'",
    );
    refused(
        &file(json!([{ "name": "same" }, { "name": "same" }])),
        "used twice",
    );
}

#[test]
fn the_limits_hold() {
    refused(
        &json!({ "version": 2, "scenes": [{ "name": "a" }] }).to_string(),
        "version 2",
    );
    refused(&file(json!([])), "no scenes");
    let many: Vec<Value> = (0..=MAX_SCENES)
        .map(|index| json!({ "name": format!("s{index}") }))
        .collect();
    refused(&file(Value::Array(many)), "more than 64 scenes");
    let steps: Vec<Value> = (0..=MAX_STEPS).map(|_| json!({ "wait_ms": 1 })).collect();
    refused(&one(Value::Array(steps)), "more than 64 steps");
    refused(&one(json!([{ "wait_ms": 30_001 }])), "wait_ms 30001");
    refused(
        &file(json!([{ "name": "a", "settle_ms": 30_001 }])),
        "settle_ms 30001",
    );
    refused(
        &json!({ "version": 1, "startup_ms": 60_001, "scenes": [{ "name": "a" }] }).to_string(),
        "startup_ms 60001",
    );
    for (width, height) in [(639, 800), (1280, 399), (3841, 800), (1280, 2161)] {
        refused(
            &json!({ "version": 1, "size": { "width": width, "height": height }, "scenes": [{ "name": "a" }] }).to_string(),
            "is outside",
        );
    }
    let arguments: Vec<Value> = (0..17)
        .map(|index| json!({ "name": format!("a{index}"), "value": "v" }))
        .collect();
    refused(
        &one(json!([{ "mcp_question": { "tool": "t", "class": "read", "arguments": arguments } }])),
        "at most 16 arguments",
    );
    refused(
        &one(
            json!([{ "mcp_question": { "tool": "t", "class": "read", "arguments": [{ "name": "a", "value": "v".repeat(501) }] } }]),
        ),
        "longer than 500",
    );
}

/// RC-100-56: `{wsl}` in a command's strings and a question's values is the distribution's name.
#[test]
fn the_wsl_placeholder_is_the_distribution() {
    let text = one(json!([
        { "command": { "command": "settings.set", "params": { "key": "appearance.wsl_themes", "value": "{wsl}\tnord-light\tnord-dark" } } },
        { "command": { "command": "panel.navigate", "params": { "panel": "right", "path": "\\\\wsl.localhost\\{wsl}\\etc" } } },
        { "mcp_question": { "tool": "ops_copy", "class": "write", "arguments": [{ "name": "target", "value": "{wsl}:{demo}" }] } }
    ]));
    let plan = parse(&text, DEMO, WSL).expect("plan");
    assert!(plan.skipped.is_empty());
    let [set, navigate, ask] = plan.scenes[0].steps.as_slice() else {
        panic!("three steps: {:?}", plan.scenes[0].steps);
    };
    assert!(matches!(
        set,
        Step::Command(command) if **command == Command::SetSetting {
            key: "appearance.wsl_themes".to_owned(),
            value: json!("Ubuntu\tnord-light\tnord-dark"),
        }
    ));
    assert!(matches!(
        navigate,
        Step::Command(command) if **command == Command::Navigate {
            panel: Some(PanelId::Right),
            path: "\\\\wsl.localhost\\Ubuntu\\etc".to_owned(),
        }
    ));
    assert!(matches!(ask, Step::Ask(ask) if ask.arguments[0].value == "Ubuntu:C:\\Demo"));
}

/// A scene without `{wsl}` is shot with or without a distribution, `{demo}` filled either way.
#[test]
fn a_scene_without_the_wsl_placeholder_needs_no_distribution() {
    let text = one(json!([
        { "command": { "command": "panel.navigate", "params": { "panel": "left", "path": "{demo}" } } }
    ]));
    for wsl in [WSL, None] {
        let plan = parse(&text, DEMO, wsl).expect("plan");
        assert!(plan.skipped.is_empty(), "{wsl:?}");
        assert!(matches!(
            &plan.scenes[0].steps[0],
            Step::Command(command) if **command == Command::Navigate {
                panel: Some(PanelId::Left),
                path: DEMO.to_owned(),
            }
        ));
    }
}

/// Without a distribution a scene naming `{wsl}` is skipped with its reason, the others are
/// shot; it is still checked, and its name still counts.
#[test]
fn without_a_distribution_a_wsl_scene_is_skipped_not_faked() {
    let text = file(json!([
        { "name": "local" },
        { "name": "wsl", "steps": [
            { "command": { "command": "panel.navigate", "params": { "path": "\\\\wsl.localhost\\{wsl}\\" } } }
        ] },
        { "name": "after" }
    ]));
    let plan = parse(&text, DEMO, None).expect("plan");
    let shot: Vec<&str> = plan
        .scenes
        .iter()
        .map(|scene| scene.name.as_str())
        .collect();
    assert_eq!(shot, ["local", "after"]);
    assert_eq!(plan.skipped.len(), 1);
    assert_eq!(plan.skipped[0].name, "wsl");
    assert_eq!(plan.skipped[0].reason, NO_WSL);

    let broken = file(json!([{ "name": "wsl", "steps": [
        { "command": { "command": "panel.navigate", "params": { "path": "{wsl}", "depth": 1 } } }
    ] }]));
    let error = parse(&broken, DEMO, None).expect_err("a skipped scene is checked too");
    assert!(
        format!("{error:#}").contains("no parameter 'depth'"),
        "{error:#}"
    );

    let twice = file(json!([
        { "name": "same", "steps": [{ "command": { "command": "panel.navigate", "params": { "path": "{wsl}" } } }] },
        { "name": "same" }
    ]));
    let error = parse(&twice, DEMO, None).expect_err("a skipped name is taken");
    assert!(format!("{error:#}").contains("used twice"), "{error:#}");
}

/// The repository's `wsl-theme` is the only scene that needs WSL; it comes last, since its
/// settings stay for any scene after it, and right after a settings scene, whose window it
/// closes.
#[test]
fn the_repository_wsl_scene_is_last_and_alone_in_needing_wsl() {
    let plan = parse(SCENES, DEMO, None).expect("scripts/screenshots/scenes.json");
    let skipped: Vec<&str> = plan.skipped.iter().map(|skip| skip.name.as_str()).collect();
    assert_eq!(skipped, ["wsl-theme"]);
    assert_eq!(
        plan.scenes.last().map(|scene| scene.capture),
        Some(Capture::Settings)
    );
    let with = parse(SCENES, DEMO, WSL).expect("scripts/screenshots/scenes.json");
    let last = with.scenes.last().expect("scenes");
    assert_eq!(last.name, "wsl-theme");
    assert!(matches!(
        last.steps.first(),
        Some(Step::Close(Capture::Settings))
    ));
}
