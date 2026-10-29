use rc_engine::{Entry, EntryKind, Owner};

use super::EntryView;

fn entry(owner: Option<Owner>) -> Entry {
    Entry {
        name: "a.txt".to_owned(),
        kind: EntryKind::File,
        size: 3,
        modified: None,
        created: None,
        attributes: Default::default(),
        hidden: false,
        owner,
    }
}

/// RC-060-02: a listing names the owner where the backend does, and says nothing elsewhere.
#[test]
fn an_entry_names_its_owner_only_where_known() {
    let owned = entry(Some(Owner {
        user: "alice".to_owned(),
        group: "staff".to_owned(),
    }));
    let json = serde_json::to_value(EntryView::from(owned)).expect("json");
    assert_eq!(
        json["owner"],
        serde_json::json!({ "user": "alice", "group": "staff" })
    );
    let json = serde_json::to_value(EntryView::from(entry(None))).expect("json");
    assert!(json.get("owner").is_none(), "{json}");
}
