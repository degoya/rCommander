//! A lease's data connections: active (`PORT`/`EPRT`) as configured, else passive, `EPSV` and
//! `PASV` one after the other.
//!
//! Servers that list `EPSV` in `FEAT` and refuse it exist - a firewall or a NAT router in front
//! of the server that only knows `PASV`, or a server on IPv6 that never learned `EPSV` - and so
//! do the reverse. A refused passive command is followed by the other one; when that works, the
//! pool keeps to it, so the refusal costs one round trip per server, not per transfer. Each
//! command's refusal carries its name, so an error says which one the server did not support.

use std::sync::atomic::Ordering;

use suppaftp::Status;

use super::Lease;
use crate::ftp::{config::DataMode, connection::Transfer, error::Failure};

impl Lease {
    /// A data connection for `command` (`RETR <path>`, `MLSD`, ...), whose refusal is `name`'s.
    /// Until [`Lease::finish`] reads its completion reply, the lease is owed one.
    pub(super) async fn open_data(
        &mut self,
        name: &'static str,
        command: &str,
    ) -> Result<Transfer, Failure> {
        let transfer = self.connect_data(name, command).await?;
        self.pending = true;
        Ok(transfer)
    }

    async fn connect_data(
        &mut self,
        name: &'static str,
        command: &str,
    ) -> Result<Transfer, Failure> {
        let (host, limit) = (self.pool.config.host.clone(), self.timeout());
        if self.pool.config.data_mode == DataMode::Active {
            let (listener, port) = command!(self, name, connection => connection.listen_active())?;
            let port_name = if port.starts_with("EPRT") {
                "EPRT"
            } else {
                "PORT"
            };
            command!(self, port_name, connection => connection.custom(&port, &[Status::CommandOk]))?;
            return command!(self, name, connection => {
                connection.open_active(listener, (name, command), &host, limit)
            });
        }
        let port = self.passive_port().await?;
        command!(self, name, connection => {
            connection.open_passive(port, (name, command), &host, limit)
        })
    }

    /// The port of a passive data connection: the pool's preferred command, else the other one.
    /// Both refused is reported as `PASV`'s refusal, which stands for passive mode as a whole.
    async fn passive_port(&mut self) -> Result<u16, Failure> {
        let epsv = self.pool.epsv.load(Ordering::Relaxed);
        match self.passive_command(epsv).await {
            Err(Failure::Reply { .. }) => {}
            done => return done,
        }
        match self.passive_command(!epsv).await {
            Ok(port) => {
                self.pool.epsv.store(!epsv, Ordering::Relaxed);
                Ok(port)
            }
            Err(Failure::Reply { code, text, .. }) => Err(Failure::Reply {
                command: "PASV",
                code,
                text,
            }),
            Err(broken) => Err(broken),
        }
    }

    async fn passive_command(&mut self, extended: bool) -> Result<u16, Failure> {
        let name = if extended { "EPSV" } else { "PASV" };
        command!(self, name, connection => connection.passive_port(extended))
    }
}
