use rc_engine::{
    AuthMethod, Connection, ConnectionId, ConnectionKind, ConnectionPatch, FolderId, SaveFields,
    SecretChange, SecretKind,
};
use rc_vault::SecretString;
use serde_json::json;

use super::{Edit, Field, Form};

/// A stored connection, built through serde so that later optional fields need no change here.
fn stored(has_password: bool) -> Connection {
    serde_json::from_value(json!({
        "id": 7, "folder_id": 2, "name": "web", "kind": "ssh", "host": "example.org", "port": 22,
        "username": "deploy", "auth_method": "password", "key_path": null,
        "login_script_id": null, "default_view": "files", "start_directory": null,
        "notes": "old", "sort_order": 0, "created_at": "", "updated_at": "",
        "has_password": has_password, "has_key": false, "has_passphrase": false, "key": null
    }))
    .expect("a connection")
}

#[test]
fn a_new_form_becomes_a_draft_in_its_folder() {
    let mut form = Form::new(Some(FolderId(3)));
    form.edit(Edit::Name(" db ".into()));
    form.edit(Edit::Host("db.example.org".into()));
    form.edit(Edit::Port("2222".into()));
    form.edit(Edit::User("root".into()));
    let plan = form.plan().expect("valid");
    let SaveFields::Create {
        folder_id,
        connection: draft,
    } = plan.fields
    else {
        panic!("a new connection");
    };
    assert_eq!(folder_id, Some(FolderId(3)));
    assert_eq!(draft.name, "db");
    assert_eq!(draft.port, Some(2222));
    assert_eq!(draft.username.as_deref(), Some("root"));
    assert_eq!(draft.notes, None);
    assert!(plan.secrets.is_empty());
    // A connection in a folder is no server of the drive bar unless marked.
    assert_eq!(plan.favourite, None);
}

#[test]
fn invalid_fields_are_reported_at_the_field_and_nothing_is_planned() {
    let mut form = Form::new(None);
    form.edit(Edit::Host("bad host".into()));
    form.edit(Edit::Port("70000".into()));
    let invalid = form.plan().expect_err("invalid");
    assert_eq!(
        invalid,
        [
            (Field::Name, "name_empty"),
            (Field::Host, "host_invalid"),
            (Field::Port, "port_invalid")
        ]
    );
    form.errors = vec![(Field::Host, "x".into())];
    form.edit(Edit::Host("ok.example.org".into()));
    assert_eq!(
        form.error(Field::Host),
        None,
        "typing clears the field's error"
    );
}

#[test]
fn a_change_sends_only_the_fields_that_changed() {
    let mut form = Form::of(&stored(false), false);
    let plan = form.plan().expect("valid");
    let SaveFields::Update { patch, .. } = &plan.fields else {
        panic!("a change");
    };
    assert_eq!(*patch, ConnectionPatch::default(), "no field changed");
    assert_eq!(plan.login_script, None, "the script did not change");
    form.edit(Edit::Host("new.example.org".into()));
    form.edit(Edit::Notes(String::new()));
    let plan = form.plan().expect("valid");
    let SaveFields::Update { id, patch } = plan.fields else {
        panic!("a change");
    };
    assert_eq!(id, ConnectionId(7));
    assert_eq!(patch.host.as_deref(), Some("new.example.org"));
    assert_eq!(
        patch.notes.as_deref(),
        Some(""),
        "an emptied text clears it"
    );
    assert_eq!(patch.name, None);
    assert_eq!(patch.username, None);
}

#[test]
fn a_stored_secret_is_never_in_the_form_and_a_typed_one_goes_to_the_plan() {
    let mut form = Form::of(&stored(true), false);
    assert!(form.stored.has_password);
    assert_eq!(rc_vault::ExposeSecret::expose_secret(&form.password), "");
    assert!(form.fields().contains(&Field::Forget(SecretKind::Password)));
    assert!(form.plan().expect("valid").secrets.is_empty());

    form.secret(SecretKind::Password, SecretString::from("hunter2"));
    let plan = form.plan().expect("valid");
    assert!(plan.stores_secrets());
    assert!(matches!(
        plan.secrets.as_slice(),
        [SecretChange::Set {
            kind: SecretKind::Password,
            ..
        }]
    ));
    assert!(!format!("{plan:?}").contains("hunter2"), "{plan:?}");
}

#[test]
fn a_key_file_is_taken_in_only_when_asked_and_a_removal_needs_no_vault() {
    let mut form = Form::of(&stored(true), false);
    form.set_auth(AuthMethod::Key);
    form.edit(Edit::KeyPath("C:\\keys\\id_ed25519".into()));
    form.secret(SecretKind::Passphrase, SecretString::from("words"));
    assert!(matches!(
        form.plan().expect("valid").secrets.as_slice(),
        [SecretChange::Set {
            kind: SecretKind::Passphrase,
            ..
        }]
    ));
    form.toggle(Field::ImportKey);
    assert!(matches!(
        form.plan().expect("valid").secrets.as_slice(),
        [SecretChange::ImportKey {
            passphrase: Some(_),
            ..
        }]
    ));

    let mut form = Form::of(&stored(true), false);
    form.toggle(Field::Forget(SecretKind::Password));
    let plan = form.plan().expect("valid");
    assert!(matches!(
        plan.secrets.as_slice(),
        [SecretChange::Clear {
            kind: SecretKind::Password
        }]
    ));
    assert!(!plan.stores_secrets());
}

#[test]
fn another_protocol_takes_its_default_port_and_the_mark_is_sent_when_changed() {
    let mut form = Form::new(None);
    assert_eq!(form.port, "22");
    assert!(form.favourite, "a root connection is a server by default");
    let other = ConnectionKind::ALL
        .iter()
        .copied()
        .find(|kind| *kind != ConnectionKind::Ssh);
    if let Some(other) = other {
        form.set_kind(other);
        assert_eq!(form.port, other.default_port().to_string());
    }
    form.edit(Edit::Port("2200".into()));
    form.set_kind(ConnectionKind::Ssh);
    assert_eq!(form.port, "2200", "a typed port stays");

    let mut form = Form::of(&stored(false), false);
    form.toggle(Field::Favourite);
    assert_eq!(form.plan().expect("valid").favourite, Some(true));
}

/// One refusal of the whole save stands at the field it is about, as each step's did.
#[test]
fn a_refusal_of_the_save_stands_at_its_field() {
    let mut form = Form::of(&stored(false), false);
    assert_eq!(form.field_of("name_taken"), Some(Field::Name));
    assert_eq!(form.field_of("key_path_refused"), Some(Field::KeyPath));
    assert_eq!(
        form.field_of("key_passphrase_wrong"),
        Some(Field::Passphrase)
    );
    assert_eq!(form.field_of("login_script_not_found"), Some(Field::Script));
    assert_eq!(form.field_of("database_error"), None);
    form.secret(SecretKind::Password, SecretString::from("   "));
    assert_eq!(form.field_of("secret_empty"), Some(Field::Password));
    form.secret(SecretKind::Password, SecretString::from("hunter2"));
    form.set_auth(AuthMethod::Key);
    form.secret(SecretKind::Passphrase, SecretString::from(" "));
    assert_eq!(form.field_of("secret_empty"), Some(Field::Passphrase));
}

/// A connection's themes (RC-100-51): loaded with it, sent only when changed, Default sent as
/// `""`; a new connection's draft carries them; each list has an id of its own.
#[test]
fn the_themes_of_each_mode_go_into_the_patch_and_the_draft() {
    let mut stored = stored(false);
    stored.theme_dark = Some("nord-dark".into());
    let mut form = Form::of(&stored, false);
    assert_eq!(form.theme(true), Some("nord-dark"));
    assert_eq!(form.theme(false), None);
    assert!(form.fields().contains(&Field::ThemeLight));
    assert!(form.fields().contains(&Field::ThemeDark));
    let unchanged = form.plan().expect("valid");
    let SaveFields::Update { patch, .. } = &unchanged.fields else {
        panic!("a change");
    };
    assert_eq!(*patch, ConnectionPatch::default());

    form.set_theme(false, Some("classic-light".into()));
    form.set_theme(true, None);
    let plan = form.plan().expect("valid");
    let SaveFields::Update { patch, .. } = plan.fields else {
        panic!("a change");
    };
    assert_eq!(patch.theme_light.as_deref(), Some("classic-light"));
    assert_eq!(patch.theme_dark.as_deref(), Some(""), "Default clears");
    assert!(patch.only_themes());

    let mut new = Form::new(None);
    new.edit(Edit::Name("db".into()));
    new.edit(Edit::Host("db.example.org".into()));
    new.set_theme(true, Some("solarized-dark".into()));
    let SaveFields::Create {
        connection: draft, ..
    } = new.plan().expect("valid").fields
    else {
        panic!("a new connection");
    };
    assert_eq!(draft.theme_dark.as_deref(), Some("solarized-dark"));
    assert_eq!(draft.theme_light, None);
    assert_ne!(Field::ThemeLight.id(), Field::ThemeDark.id());
    assert_eq!(Field::of_code("theme_wrong_mode"), Some(Field::ThemeLight));
}
