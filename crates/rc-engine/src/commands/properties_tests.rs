use std::{
    fs,
    time::{Duration, SystemTime},
};

#[cfg(unix)]
use rc_vfs::EntryKind;
use rc_vfs::{LinkKind, PropertyChange};

use crate::test_support;
use crate::{ApplyTo, Command, Engine, Outcome};

async fn engine() -> (tempfile::TempDir, Engine) {
    test_support::engine(&[("dir/a", "abc"), ("dir/b", "abcd"), ("top.txt", "hello")]).await
}

fn properties(paths: &[&str], count: bool) -> Command {
    Command::GetProperties {
        panel: None,
        paths: paths.iter().map(|path| (*path).to_owned()).collect(),
        count,
    }
}

#[tokio::test]
async fn one_folder_is_counted_and_several_entries_are_summed() {
    let (_root, engine) = engine().await;
    let Outcome::Properties(report) = engine
        .dispatch(properties(&["dir"], true))
        .await
        .expect("properties")
    else {
        panic!("properties expected");
    };
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.total, None, "no sums for one entry");
    let usage = report.items[0].usage.expect("counted");
    assert_eq!((usage.bytes, usage.files), (7, 2));
    assert!(report.capabilities.links.symlink);

    let Outcome::Properties(both) = engine
        .dispatch(properties(&["dir", "top.txt"], true))
        .await
        .expect("properties")
    else {
        panic!("properties expected");
    };
    let total = both.total.expect("sums");
    assert_eq!((total.bytes, total.files, total.dirs), (12, 3, 1));
}

#[tokio::test]
async fn without_paths_the_cursor_entry_is_meant() {
    let (_root, engine) = engine().await;
    let Outcome::Properties(report) = engine
        .dispatch(properties(&[], false))
        .await
        .expect("properties")
    else {
        panic!("properties expected");
    };
    assert_eq!(report.items[0].entry.name, "dir");
    assert_eq!(report.items[0].usage, None, "not counted unless asked");
}

#[tokio::test]
async fn attributes_and_times_change_down_the_tree() {
    let (root, engine) = engine().await;
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_400_000_000);
    let outcome = engine
        .dispatch(Command::SetFileAttributes {
            panel: None,
            paths: vec!["dir".into()],
            change: PropertyChange {
                modified: Some(then),
                ..PropertyChange::default()
            },
            recursive: true,
            apply_to: ApplyTo {
                files: true,
                dirs: false,
            },
        })
        .await
        .expect("set");
    let Outcome::AttributesSet(report) = outcome else {
        panic!("a report expected");
    };
    assert_eq!(report.changed, 3);
    let modified = fs::metadata(root.path().join("dir").join("a"))
        .and_then(|meta| meta.modified())
        .expect("modified");
    assert_eq!(modified, then);
}

#[tokio::test]
async fn nothing_to_change_is_refused() {
    let (_root, engine) = engine().await;
    let error = engine
        .dispatch(Command::SetFileAttributes {
            panel: None,
            paths: vec!["top.txt".into()],
            change: PropertyChange::default(),
            recursive: false,
            apply_to: ApplyTo::default(),
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "property_invalid");
}

#[tokio::test]
async fn a_hard_link_is_made_and_the_panel_lists_it() {
    let (root, engine) = engine().await;
    let outcome = engine
        .dispatch(Command::CreateLink {
            panel: None,
            target: Some("top.txt".into()),
            link: Some("again.txt".into()),
            kind: Some(LinkKind::Hardlink),
        })
        .await
        .expect("linked");
    assert!(matches!(
        outcome,
        Outcome::LinkCreated {
            link_kind: LinkKind::Hardlink,
            ..
        }
    ));
    assert_eq!(
        fs::read(root.path().join("again.txt")).expect("read"),
        b"hello"
    );
    let listed = engine.panel(crate::PanelId::Left);
    assert!(
        listed.index_of("again.txt").is_some(),
        "the panel was read again"
    );

    let sizes = engine
        .dispatch(Command::DirSize {
            panel: None,
            paths: vec![".".into()],
        })
        .await
        .expect("counted");
    let Outcome::DirSizes { total, .. } = sizes else {
        panic!("sizes expected");
    };
    assert_eq!(
        total.bytes, 12,
        "the second name of top.txt is not counted again"
    );
    assert_eq!(total.hard_links, 1);
}

#[tokio::test]
async fn a_link_needs_its_place() {
    let (_root, engine) = engine().await;
    let error = engine
        .dispatch(Command::CreateLink {
            panel: None,
            target: None,
            link: None,
            kind: None,
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "property_invalid");
}

#[cfg(unix)]
#[tokio::test]
async fn without_a_kind_unix_makes_a_symbolic_link() {
    let (root, engine) = engine().await;
    engine
        .dispatch(Command::CreateLink {
            panel: None,
            target: Some("dir".into()),
            link: Some("to-dir".into()),
            kind: None,
        })
        .await
        .expect("linked");
    let Outcome::Properties(report) = engine
        .dispatch(properties(&["to-dir"], false))
        .await
        .expect("properties")
    else {
        panic!("properties expected");
    };
    assert!(matches!(
        report.items[0].entry.kind,
        EntryKind::Symlink { .. }
    ));
    assert!(root.path().join("to-dir").join("a").exists());
}
