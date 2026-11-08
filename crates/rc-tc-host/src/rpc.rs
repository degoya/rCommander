//! The control messages: JSON-RPC 2.0 in [`CONTROL`](crate::frame::CONTROL) frames, and the typed
//! parameters and results of the host's methods.
//!
//! The protocol is internal - between rCommander and its own host binary - and has no public
//! contract; [`PROTOCOL`] moves with every change and both sides compare it in `host.hello`.
//! Requests go one at a time: rCommander sends the next only after the answer to the last.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The protocol version both sides speak.
pub const PROTOCOL: u32 = 1;

/// The methods of the host.
pub mod method {
    /// The host's protocol, architecture and the kinds the loaded DLL offers.
    pub const HELLO: &str = "host.hello";
    /// The entries of an archive (WCX).
    pub const WCX_LIST: &str = "wcx.list";
    /// One entry of an archive as data frames on the request's stream (WCX).
    pub const WCX_EXTRACT: &str = "wcx.extract";
    /// A file's preview bitmap as data frames on the request's stream (WLX).
    pub const WLX_PREVIEW: &str = "wlx.preview";
    /// The notification that returns credit on a stream, from the receiver to the sender.
    pub const CREDIT: &str = "$/credit";
}

/// The error codes of an answer.
pub mod code {
    /// The params do not fit the method (JSON-RPC's own code).
    pub const INVALID_PARAMS: i64 = -32602;
    /// The method is unknown (JSON-RPC's own code).
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The DLL lacks the kind the method needs, or this platform loads no DLL.
    pub const UNSUPPORTED: i64 = 1;
    /// The DLL refused the call; the message carries its error.
    pub const PLUGIN: i64 = 2;
    /// The host's own file work (staging, reading the staged file) failed.
    pub const HOST_IO: i64 = 3;
}

/// One JSON-RPC 2.0 message: a request (id and method), a notification (method, no id) or an
/// answer (id, result or error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// The request's id; none on a notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// The method of a request or notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The parameters of a request or notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// A successful answer's result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// A failed answer's error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// The error of a failed answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    /// One of [`code`].
    pub code: i64,
    /// What went wrong, never a secret (the host holds none).
    pub message: String,
}

impl Message {
    fn bare() -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id: None,
            method: None,
            params: None,
            result: None,
            error: None,
        }
    }

    /// A request.
    pub fn request(id: u64, method: &str, params: Value) -> Self {
        Self {
            id: Some(id),
            method: Some(method.to_owned()),
            params: Some(params),
            ..Self::bare()
        }
    }

    /// A notification.
    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            method: Some(method.to_owned()),
            params: Some(params),
            ..Self::bare()
        }
    }

    /// A successful answer.
    pub fn answer(id: u64, result: Value) -> Self {
        Self {
            id: Some(id),
            result: Some(result),
            ..Self::bare()
        }
    }

    /// A failed answer.
    pub fn failure(id: u64, code: i64, message: impl Into<String>) -> Self {
        Self {
            id: Some(id),
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
            ..Self::bare()
        }
    }

    /// The message as the JSON of a control frame.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a Message holds only JSON values and strings")
    }

    /// A control frame's JSON as a message; `None` for anything that is not JSON-RPC 2.0.
    pub fn from_json(json: &[u8]) -> Option<Self> {
        serde_json::from_slice::<Self>(json)
            .ok()
            .filter(|message| message.jsonrpc == "2.0")
    }
}

/// The plugin kinds of TC, as the host reports what the DLL exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A packer (`OpenArchiveW` ...), served.
    Wcx,
    /// A lister (`ListLoadW`, `ListGetPreviewBitmapW`), served for its preview.
    Wlx,
    /// A file system (`FsInitW`), detected but not served by the spike.
    Wfx,
    /// A content plugin (`ContentGetValueW`), detected but not served by the spike.
    Wdx,
}

/// The answer to `host.hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// The host's [`PROTOCOL`].
    pub protocol: u32,
    /// The host's architecture (`x86_64`, `aarch64`).
    pub arch: String,
    /// The kinds the DLL exports.
    pub kinds: Vec<Kind>,
}

/// The params of `wcx.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListParams {
    /// The archive, an absolute local path rCommander handed over.
    pub archive: String,
}

/// One entry of an archive as the DLL reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The name inside the archive, `\`-separated as TC's plugins report it.
    pub name: String,
    /// Unpacked size in bytes.
    pub size: u64,
    /// Packed size in bytes.
    pub packed: u64,
    /// Modification time, seconds since the Unix epoch (from the DOS time), if it was valid.
    pub modified: Option<i64>,
    /// The DOS attributes (`0x10` a directory).
    pub attributes: u32,
}

/// The answer to `wcx.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// The entries in the DLL's order.
    pub entries: Vec<Entry>,
}

/// The params of `wcx.extract`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractParams {
    /// The archive, an absolute local path.
    pub archive: String,
    /// The entry's name exactly as the listing reported it.
    pub entry: String,
    /// The stream the bytes come on.
    pub stream: u32,
}

/// The answer to `wcx.extract`, after the last data frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extracted {
    /// The bytes sent on the stream.
    pub size: u64,
}

/// The params of `wlx.preview`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewParams {
    /// The file, an absolute local path.
    pub file: String,
    /// The largest width wanted, in pixels.
    pub width: u32,
    /// The largest height wanted, in pixels.
    pub height: u32,
    /// The stream the pixels come on.
    pub stream: u32,
}

/// The answer to `wlx.preview`: the pixels came on the stream as rows of BGRA, top row first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// The params of `$/credit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditParams {
    /// The stream the credit is for.
    pub stream: u32,
    /// The bytes the receiver has taken off the pipe.
    pub bytes: u32,
}

#[cfg(test)]
#[path = "rpc_tests.rs"]
mod tests;
