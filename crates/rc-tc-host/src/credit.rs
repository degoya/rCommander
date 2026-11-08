//! Credit-based flow control of a data stream: the sender may have at most [`WINDOW`] bytes on the
//! pipe that the receiver has not returned with `$/credit`. The sender waits when its credit is
//! spent; a receiver that sees more than the window, or a sender granted more than it spent, has a
//! peer that broke the protocol and ends it.

use thiserror::Error;

/// The bytes a stream may have in flight.
pub const WINDOW: u32 = 1024 * 1024;

/// A peer broke the credit rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CreditError {
    /// The sender sent more than the receiver had granted.
    #[error("{0} bytes beyond the credit window")]
    Overrun(u64),
    /// The receiver returned more than was sent.
    #[error("credit of {0} bytes that were never sent")]
    Overgrant(u64),
}

/// The sender's side of one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outbound {
    available: u64,
}

impl Default for Outbound {
    fn default() -> Self {
        Self {
            available: u64::from(WINDOW),
        }
    }
}

impl Outbound {
    /// The bytes that may be sent now.
    pub fn available(&self) -> u64 {
        self.available
    }

    /// Takes `bytes` of credit for a frame about to be sent; `false` (nothing taken) when the
    /// credit is short and the sender must wait for `$/credit`.
    pub fn spend(&mut self, bytes: u64) -> bool {
        if bytes > self.available {
            return false;
        }
        self.available -= bytes;
        true
    }

    /// Credit returned by the receiver.
    pub fn grant(&mut self, bytes: u64) -> Result<(), CreditError> {
        let available = self.available.saturating_add(bytes);
        if available > u64::from(WINDOW) {
            return Err(CreditError::Overgrant(available - u64::from(WINDOW)));
        }
        self.available = available;
        Ok(())
    }
}

/// The receiver's side of one stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Inbound {
    outstanding: u64,
}

impl Inbound {
    /// Counts a data frame's `bytes` arriving; an error when they exceed the window.
    pub fn receive(&mut self, bytes: u64) -> Result<(), CreditError> {
        let outstanding = self.outstanding.saturating_add(bytes);
        if outstanding > u64::from(WINDOW) {
            return Err(CreditError::Overrun(outstanding - u64::from(WINDOW)));
        }
        self.outstanding = outstanding;
        Ok(())
    }

    /// The receiver took `bytes` off; they go back to the sender as credit.
    pub fn release(&mut self, bytes: u64) {
        self.outstanding = self.outstanding.saturating_sub(bytes);
    }
}

#[cfg(test)]
#[path = "credit_tests.rs"]
mod tests;
