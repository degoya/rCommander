//! The shell forms the test server acts out.

use std::time::Duration;

use super::{Step, script};

#[test]
fn a_sh_c_line_becomes_its_steps() {
    assert_eq!(
        script("sh -c 'echo out; echo err >&2; sleep 0.5; exit 3'"),
        vec![
            Step::Out(b"out\n".to_vec()),
            Step::Err(b"err\n".to_vec()),
            Step::Sleep(Duration::from_millis(500)),
            Step::Exit(3),
        ]
    );
    assert_eq!(
        script("sh -c 'yes | head -c 5 >&2; kill -TERM $$'"),
        vec![
            Step::Err(b"y\ny\ny".to_vec()),
            Step::Killed("TERM".to_owned())
        ]
    );
}

#[test]
fn any_other_command_is_echoed() {
    assert_eq!(
        script("uname -a"),
        vec![Step::Out(b"ran: uname -a".to_vec())]
    );
}
