//! Finding a lease its connection: an idle one that still works, else a new login on a free
//! seat, else waiting for either.
//!
//! An idle connection the server closed is seen on its socket without a round trip
//! ([`Connection::closed`]) and dropped; one with bytes waiting, or idle past [`STALE_AFTER`],
//! answers a `NOOP` first. So a dropped connection never fails the call that would have used it.
//!
//! A server may take fewer sessions than the pool would open (pure-ftpd's "2 users (the
//! maximum)", vsftpd's `max_per_ip`, ProFTPD's `MaxClientsPerUser`). A login it refuses as one
//! too many ([`FtpError::TooManySessions`]) gives up its seat for good, so the pool keeps to the
//! number of sessions it has, and the call waits for one of them; only a server that takes not
//! even one gets the error.
//!
//! Unless the server held as many sessions for this pool before: then the one too many is a
//! session the pool just closed (a cancelled download) that the server has not let go of yet -
//! pure-ftpd takes some milliseconds - and the login is tried again after a short wait instead
//! of giving up a seat the server has.

use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use tokio::sync::OwnedSemaphorePermit;

use super::{IDLE_TIMEOUT, Idle, Pool, STALE_AFTER};
use crate::ftp::{connection::Connection, error::FtpError};

/// The waits before a refused login is tried again while the server held more sessions before:
/// 0.25, 0.5, 1 and 2 s; after the last the seat is given up as for any refusal.
const LINGER_WAITS: [Duration; 4] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

impl Pool {
    /// A working connection and its seat, waiting while every seat is taken.
    pub(super) async fn seated(
        self: &Arc<Self>,
    ) -> Result<(Connection, OwnedSemaphorePermit), FtpError> {
        let mut waits = LINGER_WAITS.iter();
        loop {
            // Registered before the idle ones are looked at, so a connection given back in
            // between is not missed.
            let returned = self.returned.notified();
            tokio::pin!(returned);
            returned.as_mut().enable();
            if let Some(found) = self.reuse().await {
                return Ok(found);
            }
            let seat = tokio::select! {
                seat = Arc::clone(&self.seats).acquire_owned() => {
                    seat.expect("the pool never closes its semaphores")
                }
                () = &mut returned => continue,
            };
            match Connection::open(&self.config, self.tls.as_ref(), Some(self.info)).await {
                Ok((connection, _)) => {
                    self.proven
                        .fetch_max(self.others_seated() + 1, Ordering::SeqCst);
                    return Ok((connection, seat));
                }
                Err(FtpError::TooManySessions(text)) if self.others_seated() > 0 => {
                    if self.others_seated() < self.proven.load(Ordering::SeqCst)
                        && let Some(wait) = waits.next()
                    {
                        tracing::debug!(%text, ?wait, "the FTP server still counts a closed session");
                        tokio::time::sleep(*wait).await;
                        continue;
                    }
                    tracing::debug!(%text, "the FTP server takes no further session; waiting for one");
                    seat.forget();
                    self.seat_count.fetch_sub(1, Ordering::SeqCst);
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// The connections holding a seat besides the caller's.
    fn others_seated(&self) -> usize {
        let count = self.seat_count.load(Ordering::SeqCst);
        count
            .saturating_sub(self.seats.available_permits())
            .saturating_sub(1)
    }

    /// An idle connection that still works, its seat with it; closed ones are dropped.
    async fn reuse(&self) -> Option<(Connection, OwnedSemaphorePermit)> {
        loop {
            let idle = self.idle().pop();
            let Idle {
                mut connection,
                seat,
                used,
                checked,
            } = idle?;
            if used.elapsed() >= IDLE_TIMEOUT {
                continue;
            }
            match connection.closed().await {
                Some(true) => continue,
                Some(false) if checked.elapsed() < STALE_AFTER => return Some((connection, seat)),
                _ => {}
            }
            let alive = tokio::time::timeout(self.config.timeout, connection.noop()).await;
            if matches!(alive, Ok(Ok(()))) {
                return Some((connection, seat));
            }
        }
    }
}

#[cfg(test)]
#[path = "seats_tests.rs"]
mod tests;
