use rc_engine::{
    Command, Connection, ConnectionId, Outcome, SecretKind, SecretSourceRef, SecretSourcesOutcome,
    secret_sources::{SecretSourceInfo, SourceEntry},
};
use serde_json::json;

use super::{SourceMsg, entry_label};
use crate::app::{
    Effect,
    connections::{ConnJob, ConnectionManager, form::Form, jobs::Then},
};

const SOURCE: &str = "plugin:019d0000-0000-7000-8000-0000000000b1";
const OTHER: &str = "plugin:019d0000-0000-7000-8000-0000000000b2";

fn stored() -> Connection {
    serde_json::from_value(json!({
        "id": 7, "folder_id": null, "name": "web", "kind": "ssh", "host": "example.org",
        "port": 22, "username": "deploy", "auth_method": "password", "key_path": null,
        "login_script_id": null, "default_view": "files", "start_directory": null,
        "notes": null, "sort_order": 0, "created_at": "", "updated_at": "",
        "has_password": false, "has_key": false, "has_passphrase": false, "key": null
    }))
    .expect("a connection")
}

fn reference(entry: &str) -> SecretSourceRef {
    SecretSourceRef {
        connection_id: ConnectionId(7),
        kind: SecretKind::Password,
        source: SOURCE.to_owned(),
        entry_id: entry.to_owned(),
        entry_label: "web (root)".to_owned(),
    }
}

/// The command each effect runs, and what its answer is for.
fn commands(effects: Vec<Effect>) -> Vec<(Then, Command)> {
    effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Connections(ConnJob::Run(then, command)) => Some((then, *command)),
            _ => None,
        })
        .collect()
}

fn answer(outcome: SecretSourcesOutcome) -> Result<Outcome, &'static str> {
    Ok(Outcome::SecretSources(Box::new(outcome)))
}

/// A manager showing connection 7 with one source installed and the references read.
fn manager() -> ConnectionManager {
    let mut manager = ConnectionManager {
        form: Some(Form::of(&stored(), false)),
        ..ConnectionManager::default()
    };
    let sources = vec![SecretSourceInfo {
        source: SOURCE.to_owned(),
        name: "Bitwarden".to_owned(),
        state: "ready".to_owned(),
        code: None,
    }];
    let follow = manager.source_answer(
        Then::Sources,
        answer(SecretSourcesOutcome::Sources {
            sources,
            mappings: Vec::new(),
        }),
    );
    assert_eq!(
        commands(follow),
        vec![(
            Then::Sources,
            Command::ListSecretSources {
                connection_id: Some(ConnectionId(7)),
            }
        )]
    );
    manager
}

#[test]
fn a_source_is_searched_and_an_entry_mapped() {
    let mut manager = manager();
    manager.on_source(SourceMsg::From(
        SecretKind::Password,
        Some(SOURCE.to_owned()),
    ));
    manager.on_source(SourceMsg::Query("web".into()));
    assert_eq!(
        commands(manager.on_source(SourceMsg::Search)),
        vec![(
            Then::SourceFound(1),
            Command::SearchSecretSource {
                source: SOURCE.to_owned(),
                query: "web".into(),
            }
        )]
    );
    manager.source_answer(
        Then::SourceFound(1),
        answer(SecretSourcesOutcome::Entries {
            source: SOURCE.to_owned(),
            entries: vec![SourceEntry {
                id: "7c17".into(),
                title: "web".into(),
                user: Some("root".into()),
                url: None,
            }],
        }),
    );
    assert_eq!(
        commands(manager.on_source(SourceMsg::Pick(0))),
        vec![(
            Then::SourceMapped,
            Command::MapSecretSource(reference("7c17"))
        )]
    );
    manager.source_answer(
        Then::SourceMapped,
        answer(SecretSourcesOutcome::Mappings {
            connection_id: ConnectionId(7),
            mappings: vec![reference("7c17")],
        }),
    );
    assert!(manager.sources.search.is_none());
    assert_eq!(
        manager.sources.mapping(SecretKind::Password),
        Some(&reference("7c17"))
    );

    // Back to the vault removes the reference.
    assert_eq!(
        commands(manager.on_source(SourceMsg::From(SecretKind::Password, None))),
        vec![(
            Then::SourceMapped,
            Command::UnmapSecretSource {
                connection_id: ConnectionId(7),
                kind: SecretKind::Password,
            }
        )]
    );
}

#[test]
fn a_failed_search_stands_at_the_search() {
    let mut manager = manager();
    manager.on_source(SourceMsg::From(
        SecretKind::Password,
        Some(SOURCE.to_owned()),
    ));
    manager.on_source(SourceMsg::Search);
    manager.source_answer(Then::SourceFound(1), Err("secret_source_locked"));
    let search = manager.sources.search.as_ref().expect("still open");
    assert!(search.error.is_some());
    assert!(manager.note.is_none());
}

fn entries(source: &str, id: &str) -> Result<Outcome, &'static str> {
    answer(SecretSourcesOutcome::Entries {
        source: source.to_owned(),
        entries: vec![SourceEntry {
            id: id.into(),
            title: "web".into(),
            user: Some("root".into()),
            url: None,
        }],
    })
}

/// A search's answer counts only for the search asked last, in its own source (RC-080-39): a
/// late answer of another source is left out, so Pick never stores its entry with this source.
#[test]
fn a_late_answer_of_another_source_is_left_out() {
    let mut manager = manager();
    manager.on_source(SourceMsg::From(
        SecretKind::Password,
        Some(SOURCE.to_owned()),
    ));
    let first = commands(manager.on_source(SourceMsg::Search));
    manager.on_source(SourceMsg::From(
        SecretKind::Password,
        Some(OTHER.to_owned()),
    ));
    let second = commands(manager.on_source(SourceMsg::Search));
    let ([(late, _)], [(current, _)]) = (first.as_slice(), second.as_slice()) else {
        panic!("one search each, got {first:?} and {second:?}");
    };
    assert_ne!(late, current);

    // The first source answers now, late; and an answer naming another source is no answer.
    manager.source_answer(*late, entries(SOURCE, "from-a"));
    manager.source_answer(*late, Err("secret_source_locked"));
    manager.source_answer(*current, entries(SOURCE, "from-a"));
    let search = manager.sources.search.as_ref().expect("open");
    assert!(search.entries.is_empty() && !search.searched && search.error.is_none());
    assert!(commands(manager.on_source(SourceMsg::Pick(0))).is_empty());

    manager.source_answer(*current, entries(OTHER, "from-b"));
    let picked = commands(manager.on_source(SourceMsg::Pick(0)));
    let [(Then::SourceMapped, Command::MapSecretSource(reference))] = picked.as_slice() else {
        panic!("the reference, got {picked:?}");
    };
    assert_eq!(
        (reference.source.as_str(), reference.entry_id.as_str()),
        (OTHER, "from-b")
    );
}

#[test]
fn without_a_source_nothing_is_read() {
    let mut manager = ConnectionManager {
        form: Some(Form::of(&stored(), false)),
        ..ConnectionManager::default()
    };
    assert!(manager.sources_follow().is_empty());
}

#[test]
fn a_label_is_title_and_user_cut_to_what_is_kept() {
    let entry = SourceEntry {
        id: "1".into(),
        title: "x".repeat(600),
        user: Some("root".into()),
        url: None,
    };
    assert_eq!(entry_label(&entry).chars().count(), 512);
    let short = SourceEntry {
        title: "web".into(),
        ..entry
    };
    assert_eq!(entry_label(&short), "web (root)");
}
