//! The OSC 7, 9;9 and 133 sequences in a program's output (`rc_term::osc`): the scanner finds the
//! same payloads however the output is cut into chunks, keeps none longer than `MAX_PAYLOAD`, and
//! no folder parsed from a payload has a control character or names a share on another machine
//! (RC-080-10).

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_term::osc::{MAX_PAYLOAD, OscEvent, OscScanner, parse_osc};

fn payloads(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    let mut scanner = OscScanner::default();
    // What the scanner passes on to the terminal is not judged here, only the payloads.
    let mut forward = Vec::new();
    for chunk in chunks {
        scanner.feed(chunk, &mut forward, |payload: &[u8]| {
            found.push(payload.to_vec());
        });
        forward.clear();
    }
    found
}

fn check(payload: &[u8], hosts: &[String]) {
    if let Some(OscEvent::Folder(folder)) = parse_osc(payload, hosts) {
        assert!(
            !folder.is_empty() && !folder.chars().any(char::is_control),
            "the folder {folder:?} passed"
        );
        assert!(
            rc_vfs::network_path(&folder).is_none(),
            "the network folder {folder:?} passed"
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let hosts = ["fuzz-host".to_owned()];
    let whole = payloads(&[data]);
    // The first byte says where the output is cut in two.
    let cut = data
        .first()
        .map_or(0, |&byte| usize::from(byte) % (data.len() + 1));
    let (head, tail) = data.split_at(cut);
    assert_eq!(
        whole,
        payloads(&[head, tail]),
        "cutting the output at {cut} changed the payloads"
    );
    for payload in &whole {
        assert!(
            payload.len() <= MAX_PAYLOAD,
            "a payload of {} bytes",
            payload.len()
        );
        check(payload, &hosts);
    }
    check(data, &hosts);
});
