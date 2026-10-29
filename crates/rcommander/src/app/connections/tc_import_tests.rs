//! The page of the import from Total Commander through `apply` and a real engine on `rc-engine`'s
//! fixtures: the settings open it and get the keys back when it closes, the preview chooses what
//! is new, the import asks for no master password until the passwords are wanted, and then they
//! go into the vault and nowhere else.

use std::{fs, path::PathBuf};

use rc_db::Db;
use rc_engine::{
    Command, Engine, ImportStatus, Outcome, VfsPath,
    import::{
        tc_password::TcPassword,
        total_commander::TcSkip,
        total_commander_report::{DEFAULT_FOLDER, TcRowStatus},
    },
};
use rc_vault::SecretString;

use super::{TcImportMsg, password_code, row_importable, row_status_code};
use crate::{
    app::{
        Effect, Message,
        connections::{ConnMsg, ConnectionManager, import::Phase, vault::Ask},
        harness::Harness,
        settings::SettingsMessage,
    },
    i18n::code_text,
};

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-engine/tests/fixtures/total_commander"
);
const CANARY: &str = "tc-canary-7f3a9e";

fn manager(harness: &Harness) -> &ConnectionManager {
    harness
        .app
        .connections
        .as_ref()
        .expect("the manager is open")
}

async fn send(harness: &mut Harness, message: TcImportMsg) {
    harness
        .send(Message::Connections(ConnMsg::TcImport(message)))
        .await;
}

#[test]
fn every_code_the_page_shows_has_a_text() {
    let statuses = [
        TcRowStatus::New,
        TcRowStatus::Created,
        TcRowStatus::Exists,
        TcRowStatus::Update,
        TcRowStatus::Updated,
        TcRowStatus::Missing,
        TcRowStatus::Skipped {
            reason: "program".into(),
        },
    ];
    let mut codes: Vec<(&str, String)> = statuses
        .iter()
        .map(|status| ("tc_import.status", row_status_code(status).to_owned()))
        .collect();
    for skip in TcSkip::ALL {
        codes.push(("tc_import.skip", skip.code().to_owned()));
    }
    // What refuses a connection before the tree sees it.
    for skip in [
        TcSkip::ProtocolUnknown,
        TcSkip::PortInvalid,
        TcSkip::TooDeep,
    ] {
        codes.push(("error", format!("tc_{}", skip.code())));
    }
    for state in [
        TcPassword::None,
        TcPassword::Present,
        TcPassword::MasterPassword,
        TcPassword::Unreadable,
    ] {
        codes.push(("tc_import.password", password_code(state).to_owned()));
    }
    for warning in [
        "wincmd_not_found",
        "ftp_ini_not_found",
        "value_too_long",
        "dir_menu_redirected",
    ] {
        codes.push(("tc_import.warning", warning.to_owned()));
    }
    for reason in [
        "none",
        "master_password",
        "unreadable",
        "anonymous",
        "not_imported",
    ] {
        codes.push(("tc_import.password_skip", reason.to_owned()));
    }
    for (prefix, code) in codes {
        assert_ne!(
            code_text(prefix, &code),
            code,
            "no text for {prefix}.{code}"
        );
    }
    assert!(row_importable(&TcRowStatus::Missing));
    assert!(!row_importable(&TcRowStatus::Exists));
}

/// Total Commander's files in a temporary folder, an engine with a database on it, and the app.
async fn fixtures() -> (tempfile::TempDir, PathBuf, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let wincmd = root.path().join("wincmd.ini");
    fs::copy(format!("{FIXTURES}/wincmd_utf16.ini"), &wincmd).expect("wincmd");
    fs::copy(
        format!("{FIXTURES}/wcx_ftp.ini"),
        root.path().join("wcx_ftp.ini"),
    )
    .expect("wcx_ftp");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    (root, wincmd, Harness::new(engine))
}

/// The settings' Import page opens the import over them, without the manager's tree; Escape
/// and Back return to the settings, and the import still fills the hotlist folder and the
/// connections (RC-050-10).
#[tokio::test]
async fn the_settings_open_the_import_and_get_the_keys_back() {
    let (_root, wincmd, mut harness) = fixtures().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    // What the Import page's button sends.
    send(&mut harness, TcImportMsg::Open).await;
    let opened = manager(&harness);
    assert!(opened.tc_import.is_some() && opened.import_only && opened.over_settings());
    harness.press("Escape").await;
    assert!(harness.app.connections.is_none(), "Escape left the import");
    assert!(harness.app.settings.is_some(), "and kept the settings");

    send(&mut harness, TcImportMsg::Open).await;
    send(
        &mut harness,
        TcImportMsg::Wincmd(wincmd.display().to_string()),
    )
    .await;
    send(&mut harness, TcImportMsg::Read).await;
    send(&mut harness, TcImportMsg::Run).await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Done, "{:?}", dialog.page.error);
    let engine = harness.app.engine().clone();
    let Outcome::Hotlist { items } = engine
        .dispatch(Command::ListHotlist {})
        .await
        .expect("hotlist")
    else {
        panic!("the hotlist");
    };
    assert!(
        items
            .iter()
            .any(|item| item.folder && item.name == DEFAULT_FOLDER)
    );
    assert!(items.len() > 1, "{items:?}");
    let Outcome::ConnectionTree { tree } = engine
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    else {
        panic!("the tree");
    };
    assert_eq!(tree.folders[0].folder.name, DEFAULT_FOLDER);
    assert!(!tree.folders[0].connections.is_empty());

    send(&mut harness, TcImportMsg::Back).await;
    assert!(harness.app.connections.is_none(), "Back left the import");
    assert!(harness.app.settings.is_some());
}

/// The manager opened before keeps its tree when the import closes.
#[tokio::test]
async fn an_open_manager_stays_when_the_import_closes() {
    let (_root, _wincmd, mut harness) = fixtures().await;
    harness.press("Ctrl+F").await;
    send(&mut harness, TcImportMsg::Open).await;
    assert!(!manager(&harness).import_only);
    send(&mut harness, TcImportMsg::Back).await;
    assert!(manager(&harness).tc_import.is_none());
}

#[tokio::test]
async fn the_passwords_wait_for_the_question_and_then_go_into_the_vault() {
    let (_root, wincmd, mut harness) = fixtures().await;
    harness.press("Ctrl+F").await;
    send(&mut harness, TcImportMsg::Open).await;
    send(
        &mut harness,
        TcImportMsg::Wincmd(wincmd.display().to_string()),
    )
    .await;
    send(&mut harness, TcImportMsg::Read).await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Ready, "{:?}", dialog.page.error);
    assert!(dialog.chosen.contains("menu1") && dialog.chosen.contains("ftp:plain ftp"));
    assert!(!dialog.chosen.contains("menu5"), "a program is not offered");

    // Without the box ticked the import stores no password and asks for nothing.
    send(&mut harness, TcImportMsg::Run).await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Done);
    assert!(manager(&harness).prompt.is_none());
    let created = dialog
        .report
        .as_ref()
        .expect("report")
        .connections
        .iter()
        .filter(|row| matches!(row.status, ImportStatus::Created { .. }))
        .count();
    assert_eq!(created, 6);

    // Asked for, the passwords wait for a master password.
    send(&mut harness, TcImportMsg::Read).await;
    send(&mut harness, TcImportMsg::Passwords(true)).await;
    send(&mut harness, TcImportMsg::Run).await;
    let prompt = manager(&harness).prompt.as_ref().expect("the prompt");
    assert_eq!(prompt.ask, Ask::Create);
    let master = || SecretString::from("master-password-1");
    harness
        .send(Message::Connections(ConnMsg::VaultInput(false, master())))
        .await;
    harness
        .send(Message::Connections(ConnMsg::VaultInput(true, master())))
        .await;
    harness
        .send(Message::Connections(ConnMsg::VaultSubmit))
        .await;
    let stored = manager(&harness)
        .tc_import
        .as_ref()
        .and_then(|dialog| dialog.stored.clone())
        .expect("the passwords answered");
    assert_eq!(stored.stored.len(), 2, "{stored:?}");
    assert!(!format!("{:?}", manager(&harness)).contains(CANARY));
    let Outcome::ConnectionTree { tree } = harness
        .app
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    else {
        panic!("the tree");
    };
    let plain = tree.folders[0]
        .connections
        .iter()
        .find(|connection| connection.name == "Plain FTP")
        .expect("Plain FTP");
    assert!(plain.secrets.has_password);
}

/// The colours and the font of a `wincmd.ini` show as rows of their own, chosen when new, and
/// the import puts them into the settings (RC-050-14).
#[tokio::test]
async fn the_colours_and_the_font_are_offered_and_imported() {
    let (root, _wincmd, mut harness) = fixtures().await;
    let look = root.path().join("look").join("wincmd.ini");
    fs::create_dir(root.path().join("look")).expect("dir");
    fs::write(
        &look,
        "[Colors]\nBackColor2=15132390\nCursorColor=128\n\
         [ColorsDark]\nThemedCursor=1\nCursorColor=255\n\
         [AllResolutions]\nFontName=Consolas\nFontSize=10\n",
    )
    .expect("wincmd");
    harness.press("Ctrl+F").await;
    send(&mut harness, TcImportMsg::Open).await;
    send(
        &mut harness,
        TcImportMsg::Wincmd(look.display().to_string()),
    )
    .await;
    send(&mut harness, TcImportMsg::Read).await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    let report = dialog.report.as_ref().expect("a preview");
    let keys: Vec<&str> = report.look.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(
        keys,
        ["colour.light.stripe", "colour.light.cursor", "font.list"]
    );
    assert!(keys.iter().all(|key| dialog.chosen.contains(*key)));
    send(&mut harness, TcImportMsg::Run).await;
    let engine = harness.app.engine().clone();
    let Outcome::Settings { settings } = engine
        .dispatch(Command::GetSettings {
            key: Some("appearance.colours.light.cursor".into()),
        })
        .await
        .expect("settings")
    else {
        panic!("the setting");
    };
    assert_eq!(settings[0].value, serde_json::json!("#800000"));
}

/// The answers of `effects` held back, arriving now: a read that comes late.
async fn arrive(harness: &mut Harness, effects: Vec<Effect>) {
    for effect in effects {
        if let Effect::Connections(job) = effect {
            let answer = job.run(harness.app.engine().clone()).await;
            harness.send(Message::Connections(answer)).await;
        }
    }
}

/// A changed path drops the preview (RC-080-39): Import takes nothing from the new file, and a
/// read of the old one that answers late is left out too.
#[tokio::test]
async fn a_changed_path_drops_the_preview_and_imports_nothing() {
    let (root, wincmd, mut harness) = fixtures().await;
    let look = root.path().join("look").join("wincmd.ini");
    fs::create_dir(root.path().join("look")).expect("dir");
    fs::write(&look, "[Colors]\nCursorColor=128\n").expect("wincmd");
    harness.press("Ctrl+F").await;
    send(&mut harness, TcImportMsg::Open).await;
    send(
        &mut harness,
        TcImportMsg::Wincmd(wincmd.display().to_string()),
    )
    .await;
    send(&mut harness, TcImportMsg::Read).await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Ready, "{:?}", dialog.page.error);
    assert!(!dialog.chosen.is_empty());
    // The first file read once more, its answer held back.
    let late = harness
        .app
        .apply(Message::Connections(ConnMsg::TcImport(TcImportMsg::Read)));

    send(
        &mut harness,
        TcImportMsg::Wincmd(look.display().to_string()),
    )
    .await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Done);
    assert!(dialog.report.is_none() && dialog.chosen.is_empty());
    send(&mut harness, TcImportMsg::Run).await;
    arrive(&mut harness, late).await;
    let dialog = manager(&harness).tc_import.as_ref().expect("the page");
    assert!(dialog.report.is_none(), "the late preview is left out");
    send(&mut harness, TcImportMsg::Run).await;

    let engine = harness.app.engine().clone();
    let Outcome::ConnectionTree { tree } = engine
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    else {
        panic!("the tree");
    };
    assert!(
        tree.connections.is_empty() && tree.folders.is_empty(),
        "{tree:?}"
    );
    let Outcome::Settings { settings } = engine
        .dispatch(Command::GetSettings {
            key: Some("appearance.colours.light.cursor".into()),
        })
        .await
        .expect("settings")
    else {
        panic!("the setting");
    };
    assert_ne!(settings[0].value, serde_json::json!("#800000"));
}
