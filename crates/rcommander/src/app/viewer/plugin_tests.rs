//! A file opened with its viewer plugin, from the window's side (RC-100-13): no plugin leaves the
//! built-in viewer, a failing one leaves it with a notice, Quick View moving on stops the plugin
//! of the file before, and a document's outputs become its views. The plugin runs themselves are
//! `rc_engine::plugins`'s tests.

use std::{fs, sync::atomic::Ordering};

use iced::Color;
use rc_engine::{
    PanelId, VfsPath,
    plugins::{OpenRequest, Output, Purpose},
};

use super::{Loaded, PluginOpened, Plugins, concerns, hex, open, plugin_theme, views};
use crate::app::{
    harness::Harness,
    viewer::{ViewerMessage, document::DocView, lister::Lister, load, quick::Quick},
};

#[test]
fn outputs_become_views_in_their_order_and_pages_wait() {
    let shown = views(&[
        Output::Highlighted,
        Output::Pages,
        Output::Tree,
        Output::Text,
    ]);
    assert_eq!(shown, [DocView::Highlighted, DocView::Tree, DocView::Text]);
    assert_eq!(
        views(&[Output::Markdown, Output::Image]),
        [DocView::Markdown, DocView::Image]
    );
}

#[test]
fn the_theme_is_told_in_the_windows_colours() {
    assert_eq!(hex(Color::from_rgb8(0x20, 0x7a, 0xff)), "#207aff");
    let dark = plugin_theme(true);
    assert!(dark.dark);
    assert!(dark.background.starts_with('#') && dark.background.len() == 7);
    assert!(!plugin_theme(false).dark);
}

#[test]
fn every_plugins_setting_but_the_colours_reloads_the_plugins() {
    assert!(concerns("plugins.enabled"));
    assert!(concerns("plugins.associations"));
    assert!(
        !concerns("plugins.viewer_token_colors"),
        "the colours are the views'"
    );
    assert!(!concerns("viewer.font_size"));
}

/// Quick View moving on drops the lister of the file before, which stops its plugin; a Lister
/// closing does the same.
#[test]
fn quick_view_moving_on_stops_the_plugin_of_the_file_before() {
    let mut quick = Quick::new(PanelId::Left);
    let mut flags = Vec::new();
    for index in 0..20 {
        let lister = Lister::new(
            index,
            PanelId::Left,
            VfsPath::local(format!("f{index}.csv")),
        );
        flags.push(lister.plugin_stop());
        quick.show_file(lister);
    }
    let (last, before) = flags.split_last().expect("twenty");
    assert!(before.iter().all(|flag| flag.load(Ordering::SeqCst)));
    assert!(!last.load(Ordering::SeqCst), "the file shown runs on");
    drop(quick);
    assert!(last.load(Ordering::SeqCst), "Quick View off stops it too");
}

#[tokio::test]
async fn without_plugins_the_built_in_viewer_opens_the_file() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::write(root.path().join("data.json"), b"{\"a\": 1}\n").expect("write");
    let harness = Harness::headless(root.path()).await;
    let lister = Lister::new(
        1,
        PanelId::Left,
        VfsPath::local(root.path().join("data.json")),
    );
    let request = OpenRequest {
        purpose: Purpose::Lister,
        theme: plugin_theme(false),
        cancel: lister.plugin_stop(),
    };
    let path = lister.path.clone();
    let loaded = open(
        harness.app.engine().clone(),
        path,
        None,
        Plugins::default(),
        request,
    )
    .await;
    let Loaded::BuiltIn(Ok(opened)) = loaded else {
        panic!("the built-in viewer");
    };
    assert_eq!(opened.file.size(), 9);
}

/// *Crash*: the plugin failed, the built-in viewer shows the file with the notice, and the
/// lister goes on as for any file.
#[tokio::test]
async fn a_failing_plugin_leaves_the_built_in_viewer_with_a_notice() {
    let root = tempfile::tempdir().expect("temp dir");
    let file = root.path().join("data.json");
    fs::write(&file, b"{\"a\": 1}\n").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    let path = VfsPath::local(file);
    harness
        .app
        .viewers
        .open
        .push(Lister::new(5, PanelId::Left, path.clone()));
    let opened = load::open(harness.app.engine().clone(), path, None).await;
    let notice = "JSON viewer could not show the file".to_owned();
    let effects = harness.app.on_viewer(ViewerMessage::Plugin(
        5,
        PluginOpened::Fallback {
            opened,
            notice: notice.clone(),
        },
    ));
    assert!(!effects.is_empty(), "the first page is read");
    let lister = harness.app.viewers.lister(5).expect("open");
    assert_eq!(lister.note.as_deref(), Some(notice.as_str()));
    assert!(lister.file.is_some(), "the built-in viewer has the file");
    assert!(lister.document.is_none());
    assert!(
        harness
            .app
            .on_viewer(ViewerMessage::Plugin(
                6,
                PluginOpened::Fallback {
                    opened: Err("not_found"),
                    notice,
                },
            ))
            .is_empty(),
        "an answer for a lister gone goes nowhere"
    );
}
