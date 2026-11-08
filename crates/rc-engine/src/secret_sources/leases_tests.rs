//! The lease book (RC-100-38): a lease stands for its connection, kind, source and entry until
//! shortly before its end; one that no longer stands is taken out to be given back; a reason is
//! trimmed, bounded and kept per connection.

use rc_plugin_host::Lease;
use rc_vault::{ExposeSecret as _, SecretString};

use super::{EXPIRY_MARGIN_SECS, Found, Leases, MAX_CHECKOUT_REASON_CHARS, checked_reason};
use crate::{ConnectionId, SecretKind, SecretSourceRef};

const NOW: u64 = 1_800_000_000;

fn reference(connection: i64, kind: SecretKind, entry: &str) -> SecretSourceRef {
    SecretSourceRef {
        connection_id: ConnectionId(connection),
        kind,
        source: "plugin:019d0000-0000-7000-8000-0000000000b1".to_owned(),
        entry_id: entry.to_owned(),
        entry_label: "web (root)".to_owned(),
    }
}

fn lease(id: &str, expires: Option<u64>) -> Lease {
    Lease {
        id: id.to_owned(),
        value: SecretString::from("lent-value"),
        expires,
    }
}

fn value(found: Found) -> Option<String> {
    match found {
        Found::Value(value) => Some(value.expose_secret().to_owned()),
        Found::Stale(_) | Found::Nothing => None,
    }
}

#[test]
fn a_lease_stands_until_shortly_before_its_end() {
    let book = Leases::default();
    let password = reference(1, SecretKind::Password, "7c17");
    assert!(matches!(book.find(&password, NOW), Found::Nothing));
    assert!(
        book.hold(&password, lease("l-1", Some(NOW + 600)))
            .is_none()
    );
    assert_eq!(
        value(book.find(&password, NOW)).as_deref(),
        Some("lent-value")
    );
    assert_eq!(book.held_by(ConnectionId(1)), 1);

    // Within the margin of its end it no longer stands: taken out, to be given back.
    let late = NOW + 600 - EXPIRY_MARGIN_SECS;
    assert!(matches!(book.find(&password, late), Found::Stale(_)));
    assert_eq!(book.held_by(ConnectionId(1)), 0);
    assert!(matches!(book.find(&password, late), Found::Nothing));
}

#[test]
fn a_lease_serves_only_its_own_entry_kind_and_connection() {
    let book = Leases::default();
    let password = reference(1, SecretKind::Password, "7c17");
    book.hold(&password, lease("l-1", None));
    assert!(matches!(
        book.find(&reference(2, SecretKind::Password, "7c17"), NOW),
        Found::Nothing
    ));
    assert!(matches!(
        book.find(&reference(1, SecretKind::Key, "7c17"), NOW),
        Found::Nothing
    ));
    // The mapping moved to another entry: the old lease goes back.
    assert!(matches!(
        book.find(&reference(1, SecretKind::Password, "9a01"), NOW),
        Found::Stale(_)
    ));
}

#[test]
fn leases_are_taken_out_per_connection_and_all_at_once() {
    let book = Leases::default();
    book.hold(&reference(1, SecretKind::Password, "a"), lease("l-1", None));
    book.hold(
        &reference(1, SecretKind::Passphrase, "b"),
        lease("l-2", None),
    );
    book.hold(&reference(2, SecretKind::Password, "c"), lease("l-3", None));
    // A second lease for the same secret replaces the first, which is given back.
    assert!(
        book.hold(&reference(2, SecretKind::Password, "c"), lease("l-4", None))
            .is_some()
    );
    assert_eq!(book.take(ConnectionId(1)).len(), 2);
    assert_eq!(book.held_by(ConnectionId(1)), 0);
    assert_eq!(book.held_by(ConnectionId(2)), 1);
    assert_eq!(book.take_all().len(), 1);
    assert_eq!(book.held_by(ConnectionId(2)), 0);
}

#[test]
fn a_reason_is_trimmed_bounded_and_kept_per_connection() {
    assert_eq!(
        checked_reason("  ticket 4711 ").expect("kept").as_deref(),
        Some("ticket 4711")
    );
    assert_eq!(checked_reason("   ").expect("nothing"), None);
    assert!(checked_reason("two\nlines").is_err());
    assert!(checked_reason(&"x".repeat(MAX_CHECKOUT_REASON_CHARS + 1)).is_err());
    assert!(checked_reason(&"x".repeat(MAX_CHECKOUT_REASON_CHARS)).is_ok());

    let book = Leases::default();
    book.set_reason(ConnectionId(1), Some("ticket 4711".to_owned()));
    assert_eq!(book.reason(ConnectionId(1)).as_deref(), Some("ticket 4711"));
    assert_eq!(book.reason(ConnectionId(2)), None);
    book.set_reason(ConnectionId(1), None);
    assert_eq!(book.reason(ConnectionId(1)), None);
}
