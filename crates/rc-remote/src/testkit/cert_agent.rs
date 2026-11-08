//! An SSH agent for the tests that also holds user certificates (RC-060-09): russh's own agent
//! server lists plain keys only. It speaks just enough of the agent protocol
//! (draft-miller-ssh-agent) for a login: the list of identities - each key, then its certificate,
//! the order OpenSSH's agent lists them in after `ssh-add` - and a signature by the key that a
//! listed key's or certificate's blob names.

use std::{io, path::Path, sync::Arc};

use russh::keys::{
    Certificate, PrivateKey, signature::Signer as _, ssh_encoding::Encode as _, ssh_key::Signature,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{UnixListener, UnixStream},
    task::JoinHandle,
};

const FAILURE: u8 = 5;
const REQUEST_IDENTITIES: u8 = 11;
const IDENTITIES_ANSWER: u8 = 12;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;

/// The longest request read; a sign request of a login is a few hundred bytes.
const MAX_REQUEST: u32 = 256 * 1024;

/// One identity the agent lists: a key's or a certificate's blob, and the key that signs for it.
struct Held {
    blob: Vec<u8>,
    comment: &'static str,
    key: Arc<PrivateKey>,
}

/// An agent holding `keys`, each with its certificate when it has one, listening on the Unix
/// socket `path` for as long as the task runs.
///
/// # Panics
///
/// The socket cannot be bound, or a key or certificate does not encode.
pub fn start_cert_agent(path: &Path, keys: &[(PrivateKey, Option<Certificate>)]) -> JoinHandle<()> {
    let mut held = Vec::new();
    for (key, certificate) in keys {
        let key = Arc::new(key.clone());
        held.push(Held {
            blob: key.public_key().to_bytes().expect("encode the key"),
            comment: "key",
            key: Arc::clone(&key),
        });
        if let Some(certificate) = certificate {
            held.push(Held {
                blob: certificate.to_bytes().expect("encode the certificate"),
                comment: "certificate",
                key,
            });
        }
    }
    let held = Arc::new(held);
    let listener = UnixListener::bind(path).expect("bind the agent socket");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let held = Arc::clone(&held);
            tokio::spawn(async move {
                // A client that hangs up ends its connection; nothing to report.
                let _ = serve(stream, &held).await;
            });
        }
    })
}

/// Answers one client's requests until it hangs up.
async fn serve(mut stream: UnixStream, held: &[Held]) -> io::Result<()> {
    loop {
        let length = stream.read_u32().await?;
        if length > MAX_REQUEST {
            return Err(io::Error::other("request too long"));
        }
        let mut request = vec![0; usize::try_from(length).map_err(io::Error::other)?];
        stream.read_exact(&mut request).await?;
        let reply = answer(&request, held);
        let reply_length = u32::try_from(reply.len()).map_err(io::Error::other)?;
        stream.write_u32(reply_length).await?;
        stream.write_all(&reply).await?;
    }
}

/// The reply to one request.
fn answer(request: &[u8], held: &[Held]) -> Vec<u8> {
    let Some((&kind, mut rest)) = request.split_first() else {
        return vec![FAILURE];
    };
    match kind {
        REQUEST_IDENTITIES => {
            let mut reply = vec![IDENTITIES_ANSWER];
            put_u32(&mut reply, held.len());
            for identity in held {
                put_string(&mut reply, &identity.blob);
                put_string(&mut reply, identity.comment.as_bytes());
            }
            reply
        }
        SIGN_REQUEST => {
            let (Some(blob), Some(data)) = (take_string(&mut rest), take_string(&mut rest)) else {
                return vec![FAILURE];
            };
            let Some(identity) = held.iter().find(|identity| identity.blob == blob) else {
                return vec![FAILURE];
            };
            let signed: Result<Signature, _> = identity.key.try_sign(data);
            let Some(encoded) = signed
                .ok()
                .and_then(|signature| signature.encode_vec().ok())
            else {
                return vec![FAILURE];
            };
            let mut reply = vec![SIGN_RESPONSE];
            put_string(&mut reply, &encoded);
            reply
        }
        _ => vec![FAILURE],
    }
}

fn put_u32(out: &mut Vec<u8>, value: usize) {
    let value = u32::try_from(value).expect("a test agent holds few identities");
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    put_u32(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// The next length-prefixed string of `input`, which moves past it.
fn take_string<'a>(input: &mut &'a [u8]) -> Option<&'a [u8]> {
    let (length, rest) = input.split_first_chunk::<4>()?;
    let length = usize::try_from(u32::from_be_bytes(*length)).ok()?;
    let (value, rest) = rest.split_at_checked(length)?;
    *input = rest;
    Some(value)
}
