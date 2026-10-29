//! The agent transports on Windows: the OpenSSH agent service's named pipe and Pageant, both
//! through russh's client (Pageant's window and pipe via the `pageant` crate), so nothing here
//! needs `unsafe`. A Unix socket is not used on Windows; asking for one answers "not found".

use std::{io, path::Path};

use russh::keys::{Error, agent::client::AgentClient};

use super::AgentConnection;

pub(super) async fn connect_socket(_path: &Path) -> Result<AgentConnection, Error> {
    Err(Error::IO(io::Error::new(
        io::ErrorKind::Unsupported,
        "a Unix socket agent (SSH_AUTH_SOCK) is not used on Windows",
    )))
}

pub(super) async fn connect_pipe(name: &str) -> Result<AgentConnection, Error> {
    Ok(AgentClient::connect_named_pipe(name).await?.dynamic())
}

pub(super) async fn connect_pageant() -> Result<AgentConnection, Error> {
    Ok(AgentClient::connect_pageant().await?.dynamic())
}
