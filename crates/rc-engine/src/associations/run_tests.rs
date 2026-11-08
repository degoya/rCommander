//! `file.run_association`, `%2` and the MCP gate's view of `file.open` (RC-100-91); the programs
//! are recorded in `user_commands::STARTED` instead of started. Enter's own cases are
//! `commands/open_tests.rs`'s.

use serde_json::json;
use tokio::sync::broadcast::error::TryRecvError;

use crate::{
    AskSource, Command, CommandClass, CursorTarget, Engine, EngineEvent, PanelId,
    associations::{AssociationVerb, FileAssociation, write_rules},
    recover::LockOrRecover as _,
    settings::{ASSOCIATIONS_ALL_MATCHES_KEY, FILE_ASSOCIATIONS_KEY},
    test_support,
    user_commands::{CommandRef, Launch, STARTED},
};

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        command: command.into(),
        params: params.into(),
        ..CommandRef::default()
    }
}

fn verb(name: &str, action: CommandRef) -> AssociationVerb {
    AssociationVerb {
        name: name.into(),
        action,
    }
}

async fn store(engine: &Engine, rules: &[FileAssociation]) {
    engine
        .set_protected_setting(FILE_ASSOCIATIONS_KEY, &json!(write_rules(rules)))
        .await
        .expect("rules stored");
}

fn started(program: &str) -> Vec<Launch> {
    STARTED
        .lock_or_recover("test")
        .iter()
        .filter(|launch| launch.program == program)
        .cloned()
        .collect()
}

fn run(name: Option<&str>, verb: &str) -> Command {
    Command::RunAssociation {
        panel: Some(PanelId::Left),
        path: name.map(str::to_owned),
        verb: verb.into(),
        params: None,
    }
}

/// Marks `names` in the left panel and puts the cursor on `cursor`.
fn mark(engine: &Engine, names: &[&str], cursor: &str) {
    let mut state = engine.state();
    let panel = state.panel_mut(PanelId::Left);
    for name in names {
        panel.selection.insert((*name).to_owned());
    }
    panel.cursor = panel
        .entries
        .iter()
        .position(|entry| entry.name == cursor)
        .expect("the cursor's entry");
}

/// `%2` gives every marked file, each with its folder, to one program, as Total Commander does.
#[tokio::test]
async fn percent_two_passes_all_marked_files() {
    let (root, engine) =
        test_support::engine_with_db(&[("a 1.log", ""), ("b.log", ""), ("c.txt", "")]).await;
    let rule = FileAssociation {
        mask: "*.log".into(),
        verbs: vec![verb("merge", program("assoc-merge-test", "-m %2"))],
        ..FileAssociation::default()
    };
    store(&engine, &[rule]).await;
    mark(&engine, &["a 1.log", "b.log"], "b.log");
    engine
        .dispatch(run(None, "merge"))
        .await
        .expect("the verb ran");
    let launch = started("assoc-merge-test").pop().expect("started once");
    let dir = root.path().to_string_lossy().into_owned();
    let path = |name: &str| format!("{dir}{}{name}", std::path::MAIN_SEPARATOR);
    if cfg!(windows) {
        assert_eq!(
            launch.args,
            [format!("-m \"{}\" {}", path("a 1.log"), path("b.log"))]
        );
    } else {
        assert_eq!(
            launch.args,
            ["-m".to_owned(), path("a 1.log"), path("b.log")]
        );
    }
}

#[tokio::test]
async fn a_verb_runs_for_a_named_file_and_only_where_a_rule_has_it() {
    let (_root, engine) = test_support::engine_with_db(&[("notes.log", ""), ("sub/", "")]).await;
    let rules = [
        FileAssociation {
            mask: "*.log".into(),
            verbs: vec![verb("view", program("assoc-view-test", "%N"))],
            ..FileAssociation::default()
        },
        FileAssociation {
            mask: "*.*".into(),
            verbs: vec![verb("print", program("assoc-print-test", "%1"))],
            ..FileAssociation::default()
        },
    ];
    store(&engine, &rules).await;
    engine
        .dispatch(run(Some("notes.log"), "VIEW"))
        .await
        .expect("ran");
    assert_eq!(
        started("assoc-view-test")
            .pop()
            .map(|launch| launch.args.join(" ")),
        Some("notes.log".to_owned())
    );
    // The second rule's verb only with "all matching types".
    let refused = engine.dispatch(run(Some("notes.log"), "print")).await;
    assert_eq!(
        refused.map(|_| ()).map_err(|error| error.code()),
        Err("association_unknown")
    );
    engine
        .set_protected_setting(ASSOCIATIONS_ALL_MATCHES_KEY, &json!(true))
        .await
        .expect("switched");
    engine
        .dispatch(run(Some("notes.log"), "print"))
        .await
        .expect("ran");
    assert_eq!(started("assoc-print-test").len(), 1);
    // A folder is never associated.
    let folder = engine.dispatch(run(Some("sub"), "print")).await;
    assert_eq!(
        folder.map(|_| ()).map_err(|error| error.code()),
        Err("association_unknown")
    );
}

/// `?` asks the window for the parameters, which runs `file.run_association` with them.
#[tokio::test]
async fn a_question_mark_asks_with_the_file_and_the_command() {
    let (_root, engine) = test_support::engine_with_db(&[("ask.log", "")]).await;
    let rule = FileAssociation {
        mask: "*.log".into(),
        open: Some(program("assoc-ask-test", "?-v %1")),
        ..FileAssociation::default()
    };
    store(&engine, &[rule]).await;
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("ask.log".into()),
        })
        .await
        .expect("cursor");
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::OpenFile {
            panel: None,
            path: None,
        })
        .await
        .expect("asked");
    assert!(started("assoc-ask-test").is_empty());
    let request = loop {
        match events.try_recv() {
            Ok(EngineEvent::UserCommandAsk(request)) => break request,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => panic!("no question"),
        }
    };
    let AskSource::Association { path, verb } = request.source.clone() else {
        panic!("an association's question, got {:?}", request.source);
    };
    assert_eq!(verb, "open");
    assert_eq!(request.params, "-v %1");
    engine
        .dispatch(Command::RunAssociation {
            panel: Some(request.panel),
            path: Some(path),
            verb,
            params: Some("-w %1".into()),
        })
        .await
        .expect("ran");
    let launch = started("assoc-ask-test").pop().expect("started");
    assert!(launch.args.join(" ").starts_with("-w"));
}

/// MCP's `file_open` follows the same rules; one whose `open` starts as administrator, asks or
/// runs a row stricter than `exec` makes the call the person's.
#[tokio::test]
async fn mcp_file_open_with_such_a_rule_is_the_persons() {
    let (_root, engine) = test_support::engine_with_db(&[("run.log", ""), ("plain.txt", "")]).await;
    let asks = program("assoc-mcp-test", "?%1");
    let elevated = CommandRef {
        elevated: true,
        ..program("assoc-mcp-test", "%1")
    };
    let open = |path: &str| Command::OpenFile {
        panel: Some(PanelId::Left),
        path: Some(path.into()),
    };
    for action in [asks, elevated] {
        let rule = FileAssociation {
            mask: "*.log".into(),
            open: Some(action),
            ..FileAssociation::default()
        };
        store(&engine, &[rule]).await;
        assert_eq!(
            engine.effective_class(&open("run.log")).await,
            CommandClass::GuiOnly
        );
        assert_eq!(
            engine.effective_class(&open("plain.txt")).await,
            CommandClass::Exec,
            "no rule takes it"
        );
    }
    let plain = FileAssociation {
        mask: "*.log".into(),
        open: Some(program("assoc-mcp-test", "%1")),
        ..FileAssociation::default()
    };
    store(&engine, &[plain]).await;
    assert_eq!(
        engine.effective_class(&open("run.log")).await,
        CommandClass::Exec
    );
}
