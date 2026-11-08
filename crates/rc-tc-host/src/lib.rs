//! The host process for Total Commander's plugin DLLs (spike RC-100-02, ADR 0004).
//!
//! rCommander never loads a TC DLL itself: `rc-tc-host` does, one process per DLL, 64-bit only,
//! and offers what the DLL can do over its stdin and stdout. This library is the portable part of
//! both sides, without `unsafe`:
//!
//! - [`frame`]: length-prefixed frames, JSON-RPC control or raw data on a stream;
//! - [`rpc`]: the messages and the typed params and results of the host's methods;
//! - [`credit`]: the flow control of a stream (a 1 MiB window, returned with `$/credit`);
//! - [`server`]: the host's request loop over a [`server::TcPlugin`];
//! - [`bridge`]: rCommander's side, which checks every answer as outside data;
//! - [`process`]: starting the host with a cleared environment, and killing it;
//! - [`dll`], [`wcx`]: what TC's interfaces define that needs no DLL - the PE machine, the
//!   exports of each kind, WCX's codes, its DOS times and the names an archive may report.
//!
//! The binary (`src/host/`) puts a loaded DLL behind `TcPlugin` on Windows; elsewhere it answers
//! every plugin request with "unsupported", so the protocol builds and is tested everywhere.

pub mod bridge;
pub mod credit;
pub mod dll;
pub mod frame;
pub mod process;
pub mod rpc;
pub mod server;
pub mod wcx;

#[cfg(test)]
mod test_plugin;

pub use bridge::{Bridge, BridgeError, Checked};
pub use process::HostProcess;
pub use server::{Bitmap, PluginError, Server, TcPlugin};
