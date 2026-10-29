//! A listing read as it arrives (RC-080-46): [`Lease::listing`] holds the whole answer before it
//! returns, this hands it over chunk by chunk, so a panel shows the first entries of a long
//! directory while the rest still comes. The same bounds hold: one timeout for the whole
//! listing and [`LISTING_MAX`] bytes; a failure, or a listing given up half-way, discards the
//! connection, which still owes the transfer's reply.

use tokio::{io::AsyncReadExt, time::Instant};

use super::{LISTING_MAX, Lease, too_large};
use crate::ftp::{connection::Transfer, error::Failure};

/// Bytes read from the data connection at once.
const CHUNK: usize = 64 * 1024;

/// A listing command's transfer, open.
pub(crate) struct Listing {
    transfer: Transfer,
    name: &'static str,
    deadline: Instant,
    read: u64,
}

impl Lease {
    /// Starts a listing command (`MLSD`, `LIST`, `LIST -a`); its bytes come from
    /// [`Lease::listing_chunk`], its end is [`Lease::listing_end`].
    pub async fn listing_open(&mut self, command: &'static str) -> Result<Listing, Failure> {
        let name = command.split(' ').next().unwrap_or(command);
        let transfer = self.open_data(name, command).await?;
        Ok(Listing {
            transfer,
            name,
            deadline: Instant::now() + self.timeout(),
            read: 0,
        })
    }

    /// The next bytes of `listing`, `None` at its end.
    pub async fn listing_chunk(
        &mut self,
        listing: &mut Listing,
    ) -> Result<Option<Vec<u8>>, Failure> {
        let mut chunk = vec![0; CHUNK];
        let read =
            tokio::time::timeout_at(listing.deadline, listing.transfer.read(&mut chunk)).await;
        let read = read.map(|result| {
            let count = result?;
            listing.read = listing.read.saturating_add(count as u64);
            if listing.read > LISTING_MAX {
                return Err(too_large(LISTING_MAX));
            }
            Ok(count)
        });
        let read = read.map(|result| result.map_err(suppaftp::FtpError::ConnectionError));
        // The data is not the reply: `226` is still owed.
        match self.outcome(listing.name, read) {
            Ok(0) => Ok(None),
            Ok(count) => {
                chunk.truncate(count);
                Ok(Some(chunk))
            }
            Err(failure) => {
                // The transfer is dropped unfinished; the connection is not reused.
                self.discard();
                Err(failure)
            }
        }
    }

    /// Reads the completion reply of a listing read to its end.
    pub async fn listing_end(&mut self, listing: Listing) -> Result<(), Failure> {
        self.finish(listing.transfer).await
    }
}
