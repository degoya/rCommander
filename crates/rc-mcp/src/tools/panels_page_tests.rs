//! `list_dir` in pages and a panel's state with its selection cut (RC-110-61), end to end over
//! an in-process pipe.

use std::{collections::BTreeSet, fs};

use serde_json::{Value, json};

use super::SELECTION_SHOWN;
use crate::test_support::{Fixture, call};

const FILES: usize = 2500;

#[tokio::test]
async fn a_folder_of_2500_entries_comes_in_three_pages_each_entry_once() {
    let fixture = Fixture::new();
    let many = fixture.root().join("many");
    fs::create_dir(&many).expect("mkdir");
    for at in 0..FILES {
        fs::write(many.join(format!("f{at:04}.txt")), b"").expect("write");
    }
    let other = fixture.root().join("other");
    fs::create_dir(&other).expect("mkdir");
    fs::write(other.join("x.txt"), b"x").expect("write");
    let built = fixture.build().await;
    let client = built.client().await;
    let path = many.display().to_string();

    let mut names = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let mut arguments = json!({ "path": path });
        if let Some(cursor) = &cursor {
            arguments["cursor"] = Value::from(cursor.clone());
        }
        let page = call(&client, "list_dir", arguments).await;
        pages += 1;
        let entries = page["entries"].as_array().expect("entries");
        assert!(entries.len() <= 1000, "{}", entries.len());
        names.extend(
            entries
                .iter()
                .map(|entry| entry["name"].as_str().expect("name").to_owned()),
        );
        assert_eq!(
            page["total"].as_u64(),
            Some(FILES as u64),
            "{}",
            page["total"]
        );
        match page["next_cursor"].as_str() {
            Some(next) => {
                assert_eq!(page["truncated"], true);
                cursor = Some(next.to_owned());
            }
            None => {
                assert_eq!(page["truncated"], false);
                break;
            }
        }
    }
    assert_eq!(pages, 3);
    let expected: BTreeSet<String> = (0..FILES).map(|at| format!("f{at:04}.txt")).collect();
    assert_eq!(names.len(), FILES, "nothing twice");
    assert_eq!(names.iter().cloned().collect::<BTreeSet<_>>(), expected);

    // A cursor of this folder pages no other one.
    let first = call(&client, "list_dir", json!({ "path": path, "limit": 10 })).await;
    assert_eq!(first["entries"].as_array().map(Vec::len), Some(10));
    let cursor = first["next_cursor"].as_str().expect("a cursor").to_owned();
    let foreign = call(
        &client,
        "list_dir",
        json!({ "path": other.display().to_string(), "cursor": cursor }),
    )
    .await;
    assert_eq!(foreign["code"], "invalid_params", "{foreign}");
    let next = call(
        &client,
        "list_dir",
        json!({ "path": path, "limit": 10, "cursor": cursor }),
    )
    .await;
    assert_eq!(
        next["entries"][0]["name"], "f0010.txt",
        "{}",
        next["entries"][0]
    );

    // A panel with every entry selected names the first SELECTION_SHOWN and counts them all.
    let shown = call(&client, "navigate", json!({ "path": path })).await;
    assert!(shown.get("code").is_none(), "{shown}");
    // A long listing may still arrive: select once all of it is there.
    for _ in 0..200 {
        let state = call(&client, "get_panel_state", json!({})).await;
        if state.get("loading").is_none() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let selected = call(&client, "panel_select", json!({ "action": "all" })).await;
    for state in [selected, call(&client, "get_panel_state", json!({})).await] {
        let selection = state["selection"].as_array().expect("selection");
        assert_eq!(selection.len(), SELECTION_SHOWN);
        assert_eq!(selection[0], "f0000.txt");
        assert_eq!(state["selected_count"].as_u64(), Some(FILES as u64));
        assert_eq!(state["selection_truncated"], true);
    }
}

#[tokio::test]
async fn a_small_selection_is_named_whole() {
    let fixture = Fixture::new();
    fs::write(fixture.root().join("a.txt"), b"a").expect("write");
    let built = fixture.build().await;
    let client = built.client().await;
    let state = call(&client, "toggle_selection", json!({ "names": ["a.txt"] })).await;
    assert_eq!(state["selection"], json!(["a.txt"]), "{state}");
    assert_eq!(state["selected_count"], 1);
    assert!(state.get("selection_truncated").is_none(), "{state}");
}
