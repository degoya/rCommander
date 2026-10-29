use std::collections::HashMap;

use super::types::{Failure, FailureKind, Http, HttpResponse, SecretField, SourceState};
use super::{BRIDGE, get, search, status};

/// A bridge that answers the paths it was given and fails every other request, as a request to a
/// bridge that is not running does.
#[derive(Default)]
struct Bridge {
    answers: HashMap<String, (u16, &'static str)>,
    asked: Vec<String>,
}

impl Bridge {
    fn answering(mut self, path: &str, status: u16, body: &'static str) -> Self {
        self.answers
            .insert(format!("{BRIDGE}{path}"), (status, body));
        self
    }
}

impl Http for Bridge {
    fn get(&mut self, url: &str) -> Result<HttpResponse, Failure> {
        self.asked.push(url.to_owned());
        let (status, body) = self
            .answers
            .get(url)
            .copied()
            .ok_or(Failure::of(FailureKind::Io))?;
        Ok(HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        })
    }
}

#[test]
fn the_state_follows_the_bridge() {
    let mut ready = Bridge::default().answering("/status", 200, "ready\n");
    assert_eq!(status(&mut ready), SourceState::Ready);
    let mut locked = Bridge::default().answering("/status", 200, "locked");
    assert_eq!(status(&mut locked), SourceState::Locked);
    assert_eq!(status(&mut Bridge::default()), SourceState::Unreachable);
}

#[test]
fn a_search_reads_one_entry_per_line_up_to_the_limit() {
    let mut bridge = Bridge::default().answering(
        "/search?q=srv%2001",
        200,
        "7c17\tsrv01 root\troot\tssh://srv01\n8d28\tsrv01 backup\n\tno id\n9e39\tthird",
    );
    let found = search(&mut bridge, "srv 01", 2).expect("found");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id, "7c17");
    assert_eq!(found[0].user.as_deref(), Some("root"));
    assert_eq!(found[0].url.as_deref(), Some("ssh://srv01"));
    assert_eq!(found[1].title, "srv01 backup");
    assert_eq!(found[1].user, None);
}

#[test]
fn a_value_is_read_by_entry_and_field() {
    let mut bridge = Bridge::default()
        .answering("/secret/7c17/password", 200, "s3cret")
        .answering("/secret/8d28/totp", 404, "");
    assert_eq!(
        get(&mut bridge, "7c17", SecretField::Password).expect("value"),
        "s3cret"
    );
    assert_eq!(
        get(&mut bridge, "8d28", SecretField::Totp)
            .expect_err("missing")
            .kind,
        FailureKind::NotFound
    );
    assert_eq!(
        get(&mut bridge, "a/b", SecretField::Password)
            .expect_err("not answered")
            .kind,
        FailureKind::Io
    );
    assert!(
        bridge
            .asked
            .last()
            .is_some_and(|url| url.ends_with("/secret/a%2Fb/password"))
    );
}
