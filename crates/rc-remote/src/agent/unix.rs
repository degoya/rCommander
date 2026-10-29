//! The agent transports on Unix: the socket in `SSH_AUTH_SOCK`. The Windows pipe and Pageant
//! are not there; asking for them answers "not found".

use std::{io, path::Path};

use russh::keys::{Error, agent::client::AgentClient};

use super::AgentConnection;

pub(super) async fn connect_socket(path: &Path) -> Result<AgentConnection, Error> {
    Ok(AgentClient::connect_uds(path).await?.dynamic())
}

pub(super) async fn connect_pipe(_name: &str) -> Result<AgentConnection, Error> {
    Err(windows_only("the OpenSSH agent service"))
}

pub(super) async fn connect_pageant() -> Result<AgentConnection, Error> {
    Err(windows_only("Pageant"))
}

fn windows_only(what: &str) -> Error {
    Error::IO(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{what} runs only on Windows"),
    ))
}
