//! `LocalForward [bind_address:]port host:hostport` of an ssh_config (RC-110-03): a local port
//! forward of the host. A forward to or from a Unix socket is not one rCommander keeps; a bind
//! address other than loopback is lowered to `127.0.0.1` with a warning
//! ([`super::WarningKind::ForwardBindLowered`]) - a port other computers reach is a person's
//! decision in the window, never an import's.

use std::net::IpAddr;

/// One `LocalForward` as the import takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalForwardSpec {
    /// Loopback, as written (`127.0.0.1`, `::1`); `None` for the default `127.0.0.1`.
    pub bind: Option<String>,
    pub port: u16,
    pub host: String,
    pub host_port: u16,
}

/// What a `LocalForward` line's arguments give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Parsed {
    Forward(LocalForwardSpec),
    /// A forward with its bind address lowered to loopback.
    Lowered(LocalForwardSpec),
    /// Not a TCP forward rCommander keeps: a socket path, a port out of range, a missing part.
    Invalid,
}

/// The forward the arguments `args` of a `LocalForward` line name.
pub(super) fn parse(args: &[String]) -> Parsed {
    let [listen, target, ..] = args else {
        return Parsed::Invalid;
    };
    let Some((bind, port)) = listen_part(listen) else {
        return Parsed::Invalid;
    };
    let Some((host, host_port)) = target_part(target) else {
        return Parsed::Invalid;
    };
    let spec = |bind| LocalForwardSpec {
        bind,
        port,
        host: host.clone(),
        host_port,
    };
    match bind {
        None => Parsed::Forward(spec(None)),
        Some(bind) if bind.eq_ignore_ascii_case("localhost") => Parsed::Forward(spec(None)),
        Some(bind) if bind.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()) => {
            Parsed::Forward(spec(Some(bind)))
        }
        Some(_) => Parsed::Lowered(spec(None)),
    }
}

/// `port`, `bind:port`, `[v6]:port`, `*:port` or `bind/port`.
fn listen_part(text: &str) -> Option<(Option<String>, u16)> {
    if let Ok(port) = text.parse::<u16>() {
        return (port != 0).then_some((None, port));
    }
    let (bind, port) = split_host_port(text)?;
    Some((Some(bind), port))
}

/// `host:port`, `[v6]:port` or `host/port`.
fn target_part(text: &str) -> Option<(String, u16)> {
    let (host, port) = split_host_port(text)?;
    let plain = !host.is_empty()
        && host.len() <= 255
        && !host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control());
    plain.then_some((host, port))
}

/// A host and a port from `host:port`, `[v6]:port` or `host/port`; a path is none.
fn split_host_port(text: &str) -> Option<(String, u16)> {
    let (host, port) = if let Some(rest) = text.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        (host, port)
    } else if let Some((host, port)) = text.rsplit_once(':') {
        (host, port)
    } else {
        text.split_once('/')?
    };
    let port = port.parse::<u16>().ok().filter(|port| *port != 0)?;
    (!host.is_empty() && !host.starts_with('/')).then(|| (host.to_owned(), port))
}

#[cfg(test)]
#[path = "forward_tests.rs"]
mod tests;
