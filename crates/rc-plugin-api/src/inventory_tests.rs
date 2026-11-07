use super::{MAX_HOSTS, Protocol, secret_marker};
use crate::WIT;

#[test]
fn the_protocols_are_named_as_the_contract_names_them() {
    let protocols = [Protocol::Ssh, Protocol::Sftp, Protocol::Ftp, Protocol::Ftps];
    let names: Vec<&str> = protocols.iter().map(|protocol| protocol.as_str()).collect();
    assert!(WIT.contains(&format!("enum protocol {{ {} }}", names.join(", "))));
}

#[test]
fn the_limit_is_the_one_the_contract_states() {
    assert!(WIT.contains(&format!(
        "// at most {} {:03} hosts",
        MAX_HOSTS / 1000,
        MAX_HOSTS % 1000
    )));
}

#[test]
fn a_marker_names_its_slot() {
    assert_eq!(secret_marker("api_token"), "{{secret:api_token}}");
}
