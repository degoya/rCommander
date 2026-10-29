//! A lease's commands without a data connection: one request, one reply, each run through
//! `command!`, so a call dropped half-way leaves its lease marked as owed a reply.

use suppaftp::Status;

use super::Lease;
use crate::ftp::error::{Failure, reply_text};

impl Lease {
    pub async fn cwd(&mut self, path: &str) -> Result<(), Failure> {
        command!(self, "CWD", connection => connection.cwd(path))
    }

    pub async fn mkdir(&mut self, path: &str) -> Result<(), Failure> {
        command!(self, "MKD", connection => connection.mkdir(path))
    }

    pub async fn rmdir(&mut self, path: &str) -> Result<(), Failure> {
        command!(self, "RMD", connection => connection.rmdir(path))
    }

    pub async fn remove(&mut self, path: &str) -> Result<(), Failure> {
        command!(self, "DELE", connection => connection.remove(path))
    }

    pub async fn rename(&mut self, from: &str, to: &str) -> Result<(), Failure> {
        command!(self, "RNFR", connection => connection.rename(from, to))
    }

    pub async fn size(&mut self, path: &str) -> Result<u64, Failure> {
        let size = command!(self, "SIZE", connection => connection.size(path))?;
        Ok(u64::try_from(size).unwrap_or(u64::MAX))
    }

    /// `MDTM`: the modification time as `YYYYMMDDHHMMSS`.
    pub async fn mdtm(&mut self, path: &str) -> Result<String, Failure> {
        let command = format!("MDTM {path}");
        let reply =
            command!(self, "MDTM", connection => connection.custom(&command, &[Status::File]))?;
        Ok(reply_text(&reply.body))
    }

    /// `MLST`: the fact line of one path.
    pub async fn mlst(&mut self, path: &str) -> Result<String, Failure> {
        command!(self, "MLST", connection => connection.mlst(path))
    }

    /// `MFMT YYYYMMDDHHMMSS path`.
    pub async fn mfmt(&mut self, stamp: &str, path: &str) -> Result<(), Failure> {
        let command = format!("MFMT {stamp} {path}");
        command!(self, "MFMT", connection => connection.custom(&command, &[Status::File]))
            .map(|_| ())
    }

    /// `SITE CHMOD <octal> path`.
    pub async fn chmod(&mut self, mode: u32, path: &str) -> Result<(), Failure> {
        let command = format!("CHMOD {mode:o} {path}");
        command!(self, "SITE CHMOD", connection => connection.site(&command)).map(|_| ())
    }

    pub async fn rest(&mut self, offset: u64) -> Result<(), Failure> {
        command!(self, "REST", connection => connection.rest(offset))
    }
}
