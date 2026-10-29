use super::*;

#[test]
fn the_default_order_is_agent_key_password() {
    assert_eq!(
        SessionOptions::default().auth_order,
        vec![AuthKind::Agent, AuthKind::Key, AuthKind::Password]
    );
    assert_eq!(format_order(&AuthKind::DEFAULT_ORDER), "agent,key,password");
}

#[test]
fn an_order_round_trips_through_its_stored_form() {
    let order = vec![AuthKind::Password, AuthKind::Agent];
    assert_eq!(parse_order(&format_order(&order)), Some(order));
    assert_eq!(
        parse_order(" key , agent "),
        Some(vec![AuthKind::Key, AuthKind::Agent])
    );
}

#[test]
fn an_empty_unknown_or_repeated_order_is_refused() {
    assert_eq!(parse_order(""), None);
    assert_eq!(parse_order("agent,gssapi"), None);
    assert_eq!(parse_order("key,key"), None);
}

#[test]
fn the_method_used_serializes_without_a_secret() {
    let used = AuthUsed::Key {
        fingerprint: "SHA256:abc".to_owned(),
    };
    assert_eq!(
        serde_json::to_value(&used).expect("serializes"),
        serde_json::json!({"method": "key", "fingerprint": "SHA256:abc"})
    );
    assert_eq!(
        AuthUsed::KeyboardInteractive.label(),
        "keyboard-interactive"
    );
}
