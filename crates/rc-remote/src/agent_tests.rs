use std::time::Instant as Clock;

use super::*;

#[test]
fn unix_asks_the_socket_in_ssh_auth_sock() {
    assert_eq!(
        sources_for(false, WindowsAgents::Both, Some("/tmp/agent.sock".into())),
        vec![AgentSource::UnixSocket("/tmp/agent.sock".into())]
    );
    assert!(sources_for(false, WindowsAgents::Both, None).is_empty());
    assert!(sources_for(false, WindowsAgents::Both, Some(OsString::new())).is_empty());
}

#[test]
fn windows_asks_the_openssh_service_then_pageant_by_default() {
    let sock = Some(OsString::from("/ignored/on/windows"));
    assert_eq!(
        sources_for(true, WindowsAgents::default(), sock.clone()),
        vec![AgentSource::OpenSshPipe, AgentSource::Pageant]
    );
    assert_eq!(
        sources_for(true, WindowsAgents::Openssh, sock.clone()),
        vec![AgentSource::OpenSshPipe]
    );
    assert_eq!(
        sources_for(true, WindowsAgents::Pageant, sock),
        vec![AgentSource::Pageant]
    );
}

#[test]
fn every_error_has_its_code() {
    let errors = [
        AgentError::NotFound {
            detail: "x".to_owned(),
        },
        AgentError::Timeout {
            agent: AgentSourceKind::Pageant,
        },
        AgentError::Failed {
            agent: AgentSourceKind::SshAuthSock,
            reason: "x".to_owned(),
        },
    ];
    let codes: Vec<&str> = errors.iter().map(AgentError::code).collect();
    assert_eq!(codes, AgentError::ALL_CODES);
}

#[tokio::test]
async fn without_a_source_the_answer_says_ssh_auth_sock_is_not_set() {
    let error = list_identities(&[]).await.expect_err("nothing to ask");
    assert_eq!(error.code(), AGENT_NOT_FOUND);
    assert!(
        error.to_string().contains("SSH_AUTH_SOCK is not set"),
        "{error}"
    );
}

#[tokio::test]
async fn a_missing_socket_is_not_found_at_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let started = Clock::now();
    let error = list_identities(&[AgentSource::UnixSocket(dir.path().join("none.sock"))])
        .await
        .expect_err("no agent");
    assert!(started.elapsed() < AGENT_WAIT, "{:?}", started.elapsed());
    assert_eq!(error.code(), AGENT_NOT_FOUND);
    assert!(error.to_string().contains("none.sock"), "{error}");
}

#[cfg(unix)]
#[tokio::test]
async fn the_windows_agents_are_not_found_elsewhere() {
    let error = list_identities(&[AgentSource::OpenSshPipe, AgentSource::Pageant])
        .await
        .expect_err("no Windows here");
    assert_eq!(error.code(), AGENT_NOT_FOUND);
    assert!(error.to_string().contains("only on Windows"), "{error}");
}

/// A stand-in agent on a Unix socket, speaking just enough of the protocol.
#[cfg(unix)]
mod fake {
    use std::path::Path;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixListener,
    };

    /// `SSH_AGENT_IDENTITIES_ANSWER`.
    const IDENTITIES_ANSWER: u8 = 12;

    const ED25519: &str = include_str!("../tests/fixtures/known_hosts/ed25519.pub");
    /// `ssh-keygen -lf` of the fixture key (`fingerprints.txt`).
    pub(super) const ED25519_FINGERPRINT: &str =
        "SHA256:5RiEvVR9s2eAmq1pXd0zFRd1zhBaDjgSOeBsI9EpDw0";

    #[derive(Clone, Copy)]
    pub(super) enum Reply {
        /// Answers the listing with the fixture key under this comment.
        Key(&'static str),
        /// Reads, never answers.
        Silence,
        /// Closes the connection at once.
        HangUp,
    }

    /// The fixture key's wire blob.
    pub(super) fn ed25519_blob() -> Vec<u8> {
        let base64 = ED25519.split_whitespace().nth(1).expect("a key field");
        STANDARD.decode(base64).expect("base64")
    }

    fn string(out: &mut Vec<u8>, bytes: &[u8]) {
        let len = u32::try_from(bytes.len()).expect("short");
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(bytes);
    }

    fn answer(comment: &str) -> Vec<u8> {
        let mut body = vec![IDENTITIES_ANSWER];
        body.extend_from_slice(&1_u32.to_be_bytes());
        string(&mut body, &ed25519_blob());
        string(&mut body, comment.as_bytes());
        let mut frame = Vec::new();
        string(&mut frame, &body);
        frame
    }

    /// Serves `reply` on `path` until the test ends.
    pub(super) fn serve(path: &Path, reply: Reply) {
        let listener = UnixListener::bind(path).expect("bind");
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                match reply {
                    Reply::HangUp => drop(stream),
                    Reply::Silence => {
                        tokio::spawn(async move {
                            let mut sink = Vec::new();
                            let _ = stream.read_to_end(&mut sink).await;
                        });
                    }
                    Reply::Key(comment) => {
                        let mut len = [0_u8; 4];
                        if stream.read_exact(&mut len).await.is_err() {
                            continue;
                        }
                        let mut request = vec![0_u8; u32::from_be_bytes(len) as usize];
                        if stream.read_exact(&mut request).await.is_err() {
                            continue;
                        }
                        let _ = stream.write_all(&answer(comment)).await;
                    }
                }
            }
        });
    }
}

#[cfg(unix)]
#[tokio::test]
async fn an_agent_s_keys_are_listed_by_type_comment_and_fingerprint() {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    fake::serve(&socket, fake::Reply::Key("fixture-ed25519"));
    let listing = list_identities(&[AgentSource::UnixSocket(socket)])
        .await
        .expect("lists");
    assert_eq!(listing.sources, [AgentSourceKind::SshAuthSock]);
    assert_eq!(
        listing.identities,
        [AgentIdentity {
            source: AgentSourceKind::SshAuthSock,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint_sha256: fake::ED25519_FINGERPRINT.to_owned(),
            comment: "fixture-ed25519".to_owned(),
        }]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn one_answering_agent_is_enough_and_a_key_is_listed_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (first, second) = (dir.path().join("a.sock"), dir.path().join("b.sock"));
    fake::serve(&first, fake::Reply::Key("first"));
    fake::serve(&second, fake::Reply::Key("second"));
    let listing = list_identities(&[
        AgentSource::UnixSocket(dir.path().join("none.sock")),
        AgentSource::UnixSocket(first),
        AgentSource::UnixSocket(second),
    ])
    .await
    .expect("lists");
    assert_eq!(listing.sources.len(), 2);
    assert_eq!(listing.identities.len(), 1);
    assert_eq!(listing.identities[0].comment, "first");
}

#[cfg(unix)]
#[tokio::test]
async fn an_agent_that_never_answers_times_out() {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    fake::serve(&socket, fake::Reply::Silence);
    let started = Clock::now();
    let error = list_within(
        &[AgentSource::UnixSocket(socket)],
        Duration::from_millis(200),
    )
    .await
    .expect_err("silent");
    assert!(started.elapsed() < AGENT_WAIT, "{:?}", started.elapsed());
    assert_eq!(
        error,
        AgentError::Timeout {
            agent: AgentSourceKind::SshAuthSock
        }
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_agent_that_hangs_up_failed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    fake::serve(&socket, fake::Reply::HangUp);
    let error = list_identities(&[AgentSource::UnixSocket(socket)])
        .await
        .expect_err("hung up");
    assert_eq!(error.code(), AGENT_FAILED);
}

#[cfg(unix)]
#[tokio::test]
async fn connect_opens_the_agent_the_login_signs_with() {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    fake::serve(&socket, fake::Reply::Key("fixture-ed25519"));
    let mut agent = connect(&AgentSource::UnixSocket(socket))
        .await
        .expect("connects");
    let held = agent.request_identities().await.expect("identities");
    assert_eq!(held.len(), 1);
    assert_eq!(
        held[0].public_key().to_bytes().expect("encodes"),
        fake::ed25519_blob()
    );

    let missing = connect(&AgentSource::UnixSocket(dir.path().join("none.sock"))).await;
    assert_eq!(
        missing.err().map(|error| error.code()),
        Some(AGENT_NOT_FOUND)
    );
}

/// Ends the `ssh-agent` a test started.
#[cfg(unix)]
struct Reap(std::process::Child);

#[cfg(unix)]
impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The acceptance test: a real `ssh-agent` with a key `ssh-add` gave it. Skipped, with a note,
/// where OpenSSH's client tools are not installed.
#[cfg(unix)]
#[tokio::test]
async fn a_running_ssh_agent_lists_the_key_ssh_add_gave_it() {
    use std::process::{Command, Stdio};

    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    let key = dir.path().join("id_ed25519");
    let Ok(agent) = Command::new("ssh-agent")
        .arg("-D")
        .arg("-a")
        .arg(&socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        eprintln!("skipped: ssh-agent is not installed");
        return;
    };
    let _agent = Reap(agent);
    let made = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "rc-agent-test", "-f"])
        .arg(&key)
        .status();
    if !made.is_ok_and(|status| status.success()) {
        eprintln!("skipped: ssh-keygen is not installed");
        return;
    }
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let added = Command::new("ssh-add")
        .arg(&key)
        .env("SSH_AUTH_SOCK", &socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !added.is_ok_and(|status| status.success()) {
        eprintln!("skipped: ssh-add is not installed or the agent did not start");
        return;
    }
    let printed = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(key.with_extension("pub"))
        .output()
        .expect("ssh-keygen -lf");
    let printed = String::from_utf8(printed.stdout).expect("UTF-8");
    let fingerprint = printed.split_whitespace().nth(1).expect("a fingerprint");

    let listing = list_identities(&[AgentSource::UnixSocket(socket)])
        .await
        .expect("lists");
    assert_eq!(
        listing.identities,
        [AgentIdentity {
            source: AgentSourceKind::SshAuthSock,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint_sha256: fingerprint.to_owned(),
            comment: "rc-agent-test".to_owned(),
        }]
    );
}
