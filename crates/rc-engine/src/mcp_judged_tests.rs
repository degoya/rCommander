use super::{McpJudged, check_judged_queue, with_mcp_judged};

fn judged(saved: i64, entries: Option<&[&str]>) -> McpJudged {
    McpJudged {
        queue: Some((
            saved,
            entries.map(|entries| entries.iter().map(|&entry| entry.to_owned()).collect()),
        )),
    }
}

fn entries(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|&text| text.to_owned()).collect()
}

#[test]
fn outside_an_mcp_call_nothing_is_checked() {
    assert!(check_judged_queue(1, &entries(&["a"])).is_ok());
}

#[tokio::test]
async fn only_the_list_judged_loads() {
    let same = with_mcp_judged(judged(1, Some(&["a", "b"])), async {
        check_judged_queue(1, &entries(&["a", "b"]))
    });
    assert!(same.await.is_ok());

    let changed = with_mcp_judged(judged(1, Some(&["a", "b"])), async {
        check_judged_queue(1, &entries(&["a", "c"]))
    });
    let error = changed.await.expect_err("changed");
    assert_eq!(error.code(), "queue_changed");

    // A list that was not there when the class was judged is no list to load now.
    let created = with_mcp_judged(judged(1, None), async {
        check_judged_queue(1, &entries(&["a"]))
    });
    assert!(created.await.is_err());

    let other = with_mcp_judged(judged(1, Some(&["a"])), async {
        check_judged_queue(2, &entries(&["z"]))
    });
    assert!(other.await.is_ok(), "another list was not judged");

    let nothing = with_mcp_judged(McpJudged::default(), async {
        check_judged_queue(1, &entries(&["z"]))
    });
    assert!(nothing.await.is_ok());
}
