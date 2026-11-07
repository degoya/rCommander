use rc_plugin_api::inventory::{MAX_HOSTS, Protocol};

use super::{InventoryList, hosts};
use crate::{
    bindings::inventory_world::exports::rcommander::plugin::inventory as wit, failure::codes,
};

fn raw(id: &str, address: &str) -> wit::InventoryHost {
    wit::InventoryHost {
        external_id: id.to_owned(),
        name: format!("srv-{id}"),
        address: address.to_owned(),
        port: 22,
        protocol: wit::Protocol::Ssh,
        user: Some("deploy".to_owned()),
        folder: "dc1/web".to_owned(),
        tags: vec!["prod".to_owned()],
        note: Some("rack 4".to_owned()),
    }
}

fn plain(text: &str) -> String {
    text.to_owned()
}

fn checked(list: Vec<wit::InventoryHost>) -> InventoryList {
    hosts(list, &plain).expect("a list")
}

#[test]
fn a_good_host_passes_as_it_was_sent() {
    let list = checked(vec![raw("1", "srv1.example.net")]);
    assert_eq!(list.skipped, 0);
    let host = &list.hosts[0];
    assert_eq!(host.external_id, "1");
    assert_eq!(host.address, "srv1.example.net");
    assert_eq!(host.protocol, Protocol::Ssh);
    assert_eq!(host.folder, "dc1/web");
    assert_eq!(host.user.as_deref(), Some("deploy"));
}

#[test]
fn addresses_are_names_or_ip_addresses_only() {
    for good in [
        "10.0.0.5",
        "fe80::1",
        "[2001:db8::1]",
        "pve-01.lan",
        "host_name",
    ] {
        assert_eq!(checked(vec![raw("1", good)]).hosts.len(), 1, "{good}");
    }
    for bad in [
        "",
        "ssh://srv1",
        "root@srv1",
        "srv1:22",
        "srv1/etc",
        "srv 1",
        "srv1\nInjected",
        "999.1.1.1",
        "s\u{202e}rv",
    ] {
        let list = checked(vec![raw("1", bad)]);
        assert!(list.hosts.is_empty(), "{bad:?}");
        assert_eq!(list.skipped_ids, vec!["1".to_owned()], "{bad:?}");
    }
}

/// Control characters, `..` in the folder path, port 0: each host is left out, its id kept as
/// unread so the reconciliation does not mark it gone.
#[test]
fn hostile_hosts_are_left_out_and_counted() {
    let mut control = raw("1", "srv1");
    control.name = "srv\u{1b}[31m".to_owned();
    let mut escape = raw("2", "srv2");
    escape.folder = "dc1/../../etc".to_owned();
    let mut dot = raw("3", "srv3");
    dot.folder = "./x".to_owned();
    let mut port = raw("4", "srv4");
    port.port = 0;
    let mut backslash = raw("5", "srv5");
    backslash.folder = "dc1\\web".to_owned();
    let mut deep = raw("6", "srv6");
    deep.folder = "a/b/c/d/e/f/g/h/i".to_owned();
    let mut empty_name = raw("7", "srv7");
    empty_name.folder = "a//b".to_owned();
    let mut no_id = raw("", "srv8");
    no_id.name = "fine".to_owned();
    let list = checked(vec![
        control,
        escape,
        dot,
        port,
        backslash,
        deep,
        empty_name,
        no_id,
        raw("9", "srv9"),
    ]);
    assert_eq!(list.hosts.len(), 1);
    assert_eq!(list.hosts[0].external_id, "9");
    assert_eq!(list.skipped, 8);
    assert_eq!(list.skipped_ids, ["1", "2", "3", "4", "5", "6", "7"]);
}

#[test]
fn a_second_host_under_one_id_is_left_out() {
    let list = checked(vec![raw("1", "first"), raw("1", "second")]);
    assert_eq!(list.hosts.len(), 1);
    assert_eq!(list.hosts[0].address, "first");
    assert_eq!(list.skipped, 1);
}

#[test]
fn slashes_around_a_folder_and_the_top_are_fine() {
    let mut top = raw("1", "srv1");
    top.folder = "/".to_owned();
    let mut padded = raw("2", "srv2");
    padded.folder = "/dc1/web/".to_owned();
    let list = checked(vec![top, padded]);
    assert_eq!(list.hosts[0].folder, "");
    assert_eq!(list.hosts[1].folder, "dc1/web");
}

#[test]
fn tags_and_notes_are_bounded() {
    let mut host = raw("1", "srv1");
    host.tags = (0..100).map(|tag| format!("t{tag}")).collect();
    host.tags.insert(0, " ".to_owned());
    host.note = Some("x".repeat(10_000));
    let list = checked(vec![host]);
    assert_eq!(list.hosts[0].tags.len(), super::MAX_TAGS - 1);
    let note = list.hosts[0].note.as_deref().expect("a note");
    assert!(note.chars().count() <= super::MAX_NOTE_CHARS + 1);
}

/// 10 001 hosts are refused as a whole, with the limit to say.
#[test]
fn a_list_over_the_limit_is_refused() {
    let at_limit: Vec<_> = (0..MAX_HOSTS)
        .map(|index| raw(&index.to_string(), "srv"))
        .collect();
    assert_eq!(
        hosts(at_limit, &plain).expect("at the limit").hosts.len(),
        MAX_HOSTS
    );
    let over: Vec<_> = (0..=MAX_HOSTS)
        .map(|index| raw(&index.to_string(), "srv"))
        .collect();
    let failure = hosts(over, &plain).expect_err("over the limit");
    assert_eq!(failure.code(), Some(codes::PLUGIN_INVENTORY_TOO_LARGE));
    assert_eq!(
        failure.params.get("limit").map(String::as_str),
        Some("10000")
    );
}

#[test]
fn what_a_server_echoed_is_masked() {
    let mut host = raw("1", "srv1");
    host.note = Some("token canary-token-5521".to_owned());
    let mask = |text: &str| text.replace("canary-token-5521", "[redacted]");
    let list = hosts(vec![host], &mask).expect("a list");
    assert_eq!(list.hosts[0].note.as_deref(), Some("token [redacted]"));
}
