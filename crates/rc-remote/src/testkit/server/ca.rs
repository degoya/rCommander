//! The test server's user certificates (RC-060-09), as `sshd` with `TrustedUserCAKeys` takes
//! them: a user certificate signed by the CA, valid now, naming the user among its principals.

use russh::{
    keys::{Certificate, HashAlg, PublicKey},
    server::Auth,
};

/// Whether `certificate` lets `user` in with `ca` trusted; none trusted refuses every one.
pub(super) fn accepts(ca: Option<&PublicKey>, user: &str, certificate: &Certificate) -> Auth {
    let Some(ca) = ca else {
        return Auth::reject();
    };
    let now = crate::totp::unix_now();
    let trusted = certificate.cert_type().is_user()
        && certificate
            .validate_at(now, [&ca.fingerprint(HashAlg::Sha256)])
            .is_ok()
        && certificate
            .valid_principals()
            .iter()
            .any(|name| name == user);
    if trusted {
        Auth::Accept
    } else {
        Auth::reject()
    }
}
