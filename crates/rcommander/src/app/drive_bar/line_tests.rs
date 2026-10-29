use rc_db::Db;
use rc_engine::{
    Command, Drive, DriveGroup, DriveKind, DriveState, Keymap, PanelId, PanelView, VfsPath,
    settings::{DRIVE_LINE_BUTTONS_KEY, DRIVE_LINE_VOLUME_KEY, THEME_KEY},
};

use super::{
    LineSettings, LineStep, can_step, concerns, fit_texts, keys_of, load, step_commands,
    volume_name,
};
use crate::{
    app::{Effect, Message, drive_bar::DriveMessage, harness::Harness},
    theme::ellipsis::ELLIPSIS,
};

fn drive(label: &str, group: DriveGroup, kind: DriveKind) -> Drive {
    Drive {
        id: label.to_owned(),
        group,
        kind,
        label: label.to_owned(),
        root: Some(VfsPath::local("/")),
        state: DriveState::Ready,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: None,
    }
}

#[test]
fn the_bracket_names_the_volume_the_share_the_server_or_the_distribution() {
    let mut fixed = drive("C:", DriveGroup::Drives, DriveKind::Fixed);
    assert_eq!(volume_name(&fixed), None, "a drive without a volume name");
    fixed.volume = Some(String::new());
    assert_eq!(volume_name(&fixed), None, "an empty volume name is none");
    fixed.volume = Some("System".into());
    assert_eq!(volume_name(&fixed), Some("System"));

    let mut share = drive("N:", DriveGroup::Drives, DriveKind::Network);
    share.volume = Some("DATA".into());
    share.remote = Some(r"\\nas\media".into());
    assert_eq!(
        volume_name(&share),
        Some(r"\\nas\media"),
        "as its button does"
    );

    let distro = drive("Ubuntu", DriveGroup::Wsl, DriveKind::Wsl);
    assert_eq!(volume_name(&distro), Some("Ubuntu"));
    let server = drive("web", DriveGroup::Servers, DriveKind::Server);
    assert_eq!(volume_name(&server), Some("web"));
}

#[test]
fn name_and_free_space_stay_whole_while_they_fit() {
    let (name, info) = fit_texts(Some("System"), "2.0 GiB free of 8.0 GiB", 600.0);
    assert_eq!(name.as_deref(), Some("[System]"));
    assert_eq!(info, "2.0 GiB free of 8.0 GiB");
    let (name, info) = fit_texts(None, "Not ready", 600.0);
    assert_eq!(name, None);
    assert_eq!(info, "Not ready");
}

#[test]
fn a_narrow_line_shortens_the_name_first_then_the_free_space() {
    let long = "A very long volume name that never fits a narrow panel";
    let free = "2.0 GiB free of 8.0 GiB";
    let (name, info) = fit_texts(Some(long), free, 260.0);
    let name = name.expect("a name");
    assert!(name.starts_with('[') && name.ends_with(']'), "{name}");
    assert!(name.contains(ELLIPSIS), "the name gives way: {name}");
    assert_eq!(info, free, "the free space keeps its room");

    let (name, info) = fit_texts(Some(long), free, 90.0);
    assert!(name.expect("a name").contains(ELLIPSIS));
    assert!(info.contains(ELLIPSIS), "both give way: {info}");
}

#[test]
fn the_buttons_run_the_keymap_s_root_and_parent_rows() {
    let keymap = Keymap::standard();
    for step in [LineStep::Root, LineStep::Up] {
        let rows: Vec<_> = keymap
            .bindings()
            .iter()
            .filter(|binding| binding.name == step.binding())
            .collect();
        assert!(!rows.is_empty(), "{} is a keymap row", step.binding());
        for row in rows {
            assert_eq!(row.command, step.command(None), "{}", step.binding());
        }
    }
    assert_eq!(keys_of(&keymap, "root_folder"), "Ctrl+\\");
    assert_eq!(keys_of(&keymap, "parent_folder"), "Backspace, Ctrl+PageUp");
}

#[test]
fn a_button_of_the_other_panel_activates_it_first() {
    assert_eq!(
        step_commands(LineStep::Up, PanelId::Left, PanelId::Left),
        vec![Command::Navigate {
            panel: Some(PanelId::Left),
            path: "..".into()
        }]
    );
    assert_eq!(
        step_commands(LineStep::Root, PanelId::Right, PanelId::Left),
        vec![
            Command::SwitchActivePanel {
                panel: Some(PanelId::Right)
            },
            Command::Navigate {
                panel: Some(PanelId::Right),
                path: "/".into()
            }
        ]
    );
}

#[test]
fn the_buttons_are_disabled_at_a_root_and_over_a_terminal() {
    let root = VfsPath::local("/");
    let below = VfsPath::local("/home/user");
    assert!(!can_step(&root, PanelView::Files), "nothing above a root");
    assert!(can_step(&below, PanelView::Files));
    assert!(
        !can_step(&below, PanelView::Terminal),
        "the terminal has the panel"
    );
}

#[test]
fn the_line_reads_its_own_settings_only() {
    assert!(concerns(DRIVE_LINE_BUTTONS_KEY));
    assert!(concerns(DRIVE_LINE_VOLUME_KEY));
    assert!(!concerns(THEME_KEY));
}

#[tokio::test]
async fn both_parts_show_until_a_setting_hides_them() {
    let db = Db::open_in_memory().await.expect("db");
    assert_eq!(load(db.clone()).await, LineSettings::default());
    assert_eq!(
        LineSettings::default(),
        LineSettings {
            volume: true,
            buttons: true
        }
    );
    db.set(DRIVE_LINE_BUTTONS_KEY, &false)
        .await
        .expect("stored");
    assert_eq!(
        load(db.clone()).await,
        LineSettings {
            volume: true,
            buttons: false
        }
    );
    db.set(DRIVE_LINE_VOLUME_KEY, &false).await.expect("stored");
    assert_eq!(
        load(db).await,
        LineSettings {
            volume: false,
            buttons: false
        }
    );
}

#[tokio::test]
async fn a_click_goes_up_in_its_panel_and_the_settings_reach_the_line() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let effects = harness.app.apply(Message::Drive(DriveMessage::Step(
        PanelId::Right,
        LineStep::Up,
    )));
    let dispatched: Vec<_> = effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Dispatch(panel, commands) => Some((panel, commands)),
            _ => None,
        })
        .collect();
    assert_eq!(
        dispatched,
        vec![(
            PanelId::Right,
            step_commands(LineStep::Up, PanelId::Right, harness.app.active)
        )]
    );

    let hidden = LineSettings {
        volume: false,
        buttons: false,
    };
    harness
        .send(Message::Drive(DriveMessage::Line(hidden)))
        .await;
    assert_eq!(harness.app.drive_bar.line, hidden);
}
