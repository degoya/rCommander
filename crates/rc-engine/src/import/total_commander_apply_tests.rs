use rc_vfs::VfsPath;

use super::{chosen, location, path_key, same_place};
use crate::{
    Engine, PanelId,
    import::{
        total_commander::{TcMenuItem, TcMenuKind, TcTab},
        total_commander_report::{TcRowStatus, TcTabsMode},
    },
    read_guard, test_support,
};

#[test]
fn a_row_is_chosen_by_its_key_or_by_default() {
    let only = vec!["menu1".to_owned(), "left2".to_owned()];
    assert!(chosen("menu1", Some(&only), false));
    assert!(!chosen("menu3", Some(&only), true));
    assert!(chosen("menu3", None, true));
    assert!(!chosen("left0", None, false));
}

#[test]
fn places_compare_as_folders_and_names_without_case() {
    let stored = location("C:\\Work").expect("encodes");
    assert_eq!(
        serde_json::from_str::<VfsPath>(&stored).expect("decodes"),
        VfsPath::local("C:\\Work")
    );
    assert!(same_place(Some(&stored), Some("C:\\Work")));
    assert!(!same_place(Some(&stored), Some("C:\\Other")));
    assert!(!same_place(Some(&stored), None));
    assert!(same_place(None, None));
    assert_eq!(
        path_key(&["Projects".to_owned(), "RC".to_owned()]),
        vec!["projects".to_owned(), "rc".to_owned()]
    );
}

const SHARE: &str = r"\\files.example\team\docs";

fn entry(index: usize, path: &str, target: Option<&str>) -> TcMenuItem {
    TcMenuItem {
        index,
        name: format!("Entry {index}"),
        folders: Vec::new(),
        kind: TcMenuKind::Entry {
            path: path.to_owned(),
            target: target.map(str::to_owned),
        },
    }
}

fn tab(position: usize, path: &str) -> TcTab {
    TcTab {
        panel: PanelId::Left,
        position,
        path: Ok(path.to_owned()),
        caption: None,
        locked: false,
        active: position == 0,
    }
}

fn network_skipped() -> TcRowStatus {
    TcRowStatus::Skipped {
        reason: "network_path".to_owned(),
    }
}

async fn hotlist(engine: &Engine, items: &[TcMenuItem]) -> Vec<TcRowStatus> {
    engine
        .import_tc_hotlist(items, None, true)
        .await
        .expect("planned")
        .into_iter()
        .map(|row| row.status)
        .collect()
}

/// RC-110-58: an agent's import skips a menu entry whose folder or other panel's folder is on
/// another machine's share and a tab on one, without asking whether it exists; a local folder
/// and a WSL distribution's share stay as they were.
#[tokio::test]
async fn an_agent_s_import_skips_shares_without_asking_about_them() {
    let (root, engine) = test_support::engine_with_db(&[("work/", "")]).await;
    let work = root.path().join("work").display().to_string();
    let items = [
        entry(1, SHARE, None),
        entry(2, &work, Some(SHARE)),
        entry(3, r"\\?\UNC\files.example\team", None),
        entry(4, &work, None),
        entry(5, r"\\wsl.localhost\Ubuntu\home", None),
    ];
    let statuses = read_guard::as_mcp(hotlist(&engine, &items)).await;
    assert_eq!(
        statuses,
        [
            network_skipped(),
            network_skipped(),
            network_skipped(),
            TcRowStatus::New,
            TcRowStatus::New,
        ]
    );
    let tabs = [tab(0, SHARE), tab(1, &work)];
    // `only` names the share's tab too: a skipped row is never written.
    let only = vec!["left0".to_owned(), "left1".to_owned()];
    let rows =
        read_guard::as_mcp(engine.import_tc_tabs(&tabs, Some(&only), TcTabsMode::Append, false))
            .await
            .expect("imported");
    assert_eq!(rows[0].status, network_skipped());
    assert!(!rows[0].selected);
    assert_eq!(rows[1].status, TcRowStatus::Created);
    assert!(
        engine
            .tabs(PanelId::Left)
            .iter()
            .all(|open| open.path != VfsPath::local(SHARE))
    );
}

/// The person's import in the window takes a share as before (RC-050-09).
#[tokio::test]
async fn the_person_s_import_keeps_a_share() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let statuses = hotlist(&engine, &[entry(1, SHARE, Some(SHARE))]).await;
    assert_eq!(statuses, [TcRowStatus::New]);
}
