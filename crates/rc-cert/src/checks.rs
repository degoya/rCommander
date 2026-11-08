//! The checks over a file's objects, each one [`Check`] of a level and a [`Finding`]: validity
//! against the caller's clock, the chain inside the file (names and key identifiers, order,
//! roots, CA flags), private keys against the certificates and requests, weak keys and
//! signatures. Signatures are not verified cryptographically; the list says so. A finding is
//! data: the viewer puts it into words in the interface's language.

use crate::cert::Certificate;
use crate::object::{Body, Damage, Object, Unsupported};
use crate::oids::Strength;
use crate::privkey::{NoPublic, PrivateKeyInfo};
use crate::pubkey::{PublicKey, Weakness};

/// How a check came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Error,
}

/// What a signature or a key belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signed {
    Certificate,
    Request,
}

/// What a check found. `object` and the other numbers are the report's object numbers (`#1` is
/// the first); dates are Unix seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// Valid until `until`, `days` whole days left.
    ValidUntil {
        object: usize,
        until: i64,
        days: i64,
    },
    /// Expires within the warning days.
    ExpiresSoon {
        object: usize,
        until: i64,
        days: i64,
    },
    /// Expired `days` days ago.
    Expired {
        object: usize,
        until: i64,
        days: i64,
    },
    /// Valid only from `from`, in `days` days.
    NotYetValid {
        object: usize,
        from: i64,
        days: i64,
    },
    SelfSignedRoot {
        object: usize,
    },
    IssuedBy {
        object: usize,
        issuer: usize,
    },
    /// Issued by `issuer`, which comes before it in the file.
    IssuerBefore {
        object: usize,
        issuer: usize,
    },
    /// `issuer` issues `object` but is not marked as a CA.
    IssuerNotCa {
        object: usize,
        issuer: usize,
    },
    /// `object` names `issuer`'s subject as its issuer, with other key identifiers.
    KeyIdMismatch {
        object: usize,
        issuer: usize,
    },
    /// The issuer `name` is not in the file; `broken` when the chain goes on after it.
    IssuerMissing {
        object: usize,
        name: String,
        broken: bool,
    },
    /// Signed with SHA-1 (`broken` false) or MD5/MD2 (`broken` true).
    WeakSignature {
        object: usize,
        signed: Signed,
        algorithm: String,
        broken: bool,
    },
    KeyEncrypted {
        object: usize,
        cipher: String,
    },
    KeyNotEncrypted {
        object: usize,
    },
    /// A private key without its public half beside certificates or requests.
    KeyNotComparable {
        object: usize,
        why: NoPublic,
    },
    KeyBelongs {
        object: usize,
        holder: usize,
        signed: Signed,
    },
    KeyMatchesNothing {
        object: usize,
    },
    WeakKey {
        object: usize,
        weakness: Weakness,
    },
    Unsupported {
        object: usize,
        what: Unsupported,
    },
    Damaged {
        object: usize,
        why: Damage,
    },
    /// The objects beyond the first `shown` are left out.
    MoreObjects {
        shown: usize,
    },
    /// Certificates or requests are in the file; their signatures are not verified.
    SignaturesNotVerified,
    NothingToCheck,
}

/// One check's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub finding: Finding,
}

/// The time the checks are made at and when a coming expiry warns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// Unix seconds.
    pub now: i64,
    /// Days before `not after` from which an expiry is a warning; 0 never warns.
    pub warn_days: i64,
}

/// Every check over `objects`, numbered as the report numbers them (`#1` is the first).
#[must_use]
pub fn run(objects: &[Object], clock: Clock) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut push = |level: Level, finding: Finding| checks.push(Check { level, finding });
    let certs: Vec<(usize, &Certificate)> = objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| match &object.body {
            Body::Certificate(cert) => Some((index + 1, cert.as_ref())),
            _ => None,
        })
        .collect();
    for &(number, cert) in &certs {
        validity(number, cert, clock, &mut push);
        chain(number, cert, &certs, &mut push);
        if !cert.self_signed() {
            signature(
                number,
                Signed::Certificate,
                &cert.signature.name,
                cert.signature.strength,
                &mut push,
            );
        }
    }
    for (index, object) in objects.iter().enumerate() {
        let number = index + 1;
        match &object.body {
            Body::Request(request) => signature(
                number,
                Signed::Request,
                &request.signature.name,
                request.signature.strength,
                &mut push,
            ),
            Body::PrivateKey(key) => private_key(number, key, objects, &mut push),
            Body::Unsupported(what) => push(
                Level::Info,
                Finding::Unsupported {
                    object: number,
                    what: what.clone(),
                },
            ),
            Body::Damaged(why) => push(
                Level::Error,
                Finding::Damaged {
                    object: number,
                    why: *why,
                },
            ),
            _ => {}
        }
        if let Some(key) = key_of(&object.body)
            && let Some((level, weakness)) = key.weakness()
        {
            push(
                level,
                Finding::WeakKey {
                    object: number,
                    weakness,
                },
            );
        }
    }
    let signed = objects
        .iter()
        .any(|object| matches!(object.body, Body::Certificate(_) | Body::Request(_)));
    if signed {
        push(Level::Info, Finding::SignaturesNotVerified);
    }
    if checks.is_empty() {
        checks.push(Check {
            level: Level::Ok,
            finding: Finding::NothingToCheck,
        });
    }
    checks
}

/// The public key an object carries, if any.
#[must_use]
pub fn key_of(body: &Body) -> Option<&PublicKey> {
    match body {
        Body::Certificate(cert) => cert.key.as_ref(),
        Body::Request(request) => request.key.as_ref(),
        Body::PrivateKey(key) => key.public.as_ref(),
        Body::PublicKey(key) => Some(key),
        Body::SshKey(line) => line.key.as_ref(),
        Body::Unsupported(_) | Body::Damaged(_) => None,
    }
}

fn validity(
    object: usize,
    cert: &Certificate,
    clock: Clock,
    push: &mut impl FnMut(Level, Finding),
) {
    let days = crate::show::days_between(clock.now, cert.not_after);
    let until = cert.not_after;
    if clock.now < cert.not_before {
        let days = crate::show::days_between(clock.now, cert.not_before).saturating_add(1);
        push(
            Level::Error,
            Finding::NotYetValid {
                object,
                from: cert.not_before,
                days,
            },
        );
    } else if clock.now > cert.not_after {
        let days = days.saturating_neg();
        push(
            Level::Error,
            Finding::Expired {
                object,
                until,
                days,
            },
        );
    } else if days < clock.warn_days {
        push(
            Level::Warn,
            Finding::ExpiresSoon {
                object,
                until,
                days,
            },
        );
    } else {
        push(
            Level::Ok,
            Finding::ValidUntil {
                object,
                until,
                days,
            },
        );
    }
}

fn chain(
    object: usize,
    cert: &Certificate,
    certs: &[(usize, &Certificate)],
    push: &mut impl FnMut(Level, Finding),
) {
    if cert.self_signed() {
        push(Level::Ok, Finding::SelfSignedRoot { object });
        return;
    }
    let named: Vec<&(usize, &Certificate)> = certs
        .iter()
        .filter(|(other, candidate)| *other != object && candidate.subject_raw == cert.issuer_raw)
        .collect();
    let linked = named.iter().find(|(_, candidate)| {
        match (
            &cert.extensions.authority_key_id,
            &candidate.extensions.subject_key_id,
        ) {
            (Some(authority), Some(subject)) => authority == subject,
            _ => true,
        }
    });
    match (linked, named.first()) {
        (Some(&&(issuer, issuer_cert)), _) => {
            if issuer < object {
                push(Level::Warn, Finding::IssuerBefore { object, issuer });
            } else {
                push(Level::Ok, Finding::IssuedBy { object, issuer });
            }
            if !issuer_cert.is_ca() {
                push(Level::Warn, Finding::IssuerNotCa { object, issuer });
            }
        }
        (None, Some(&&(issuer, _))) => {
            push(Level::Warn, Finding::KeyIdMismatch { object, issuer });
        }
        (None, None) => {
            let broken = !certs.last().is_some_and(|&(other, _)| other == object);
            let level = if broken { Level::Warn } else { Level::Info };
            push(
                level,
                Finding::IssuerMissing {
                    object,
                    name: cert.issuer.clone(),
                    broken,
                },
            );
        }
    }
}

fn signature(
    object: usize,
    signed: Signed,
    name: &str,
    strength: Strength,
    push: &mut impl FnMut(Level, Finding),
) {
    let broken = match strength {
        Strength::Good => return,
        Strength::Weak => false,
        Strength::Broken => true,
    };
    let level = if broken { Level::Error } else { Level::Warn };
    push(
        level,
        Finding::WeakSignature {
            object,
            signed,
            algorithm: name.to_owned(),
            broken,
        },
    );
}

fn private_key(
    object: usize,
    key: &PrivateKeyInfo,
    objects: &[Object],
    push: &mut impl FnMut(Level, Finding),
) {
    match &key.encrypted {
        Some(cipher) => push(
            Level::Info,
            Finding::KeyEncrypted {
                object,
                cipher: cipher.clone(),
            },
        ),
        None => push(Level::Info, Finding::KeyNotEncrypted { object }),
    }
    let holders: Vec<(usize, Signed, Option<&PublicKey>)> = objects
        .iter()
        .enumerate()
        .filter_map(|(index, other)| match &other.body {
            Body::Certificate(cert) => Some((index + 1, Signed::Certificate, cert.key.as_ref())),
            Body::Request(request) => Some((index + 1, Signed::Request, request.key.as_ref())),
            _ => None,
        })
        .collect();
    if holders.is_empty() {
        return;
    }
    let Some(public) = &key.public else {
        let why = key.no_public.unwrap_or(NoPublic::NotStored);
        push(Level::Warn, Finding::KeyNotComparable { object, why });
        return;
    };
    let mut matched = false;
    for (holder, signed, holder_key) in holders {
        if holder_key == Some(public) {
            matched = true;
            push(
                Level::Ok,
                Finding::KeyBelongs {
                    object,
                    holder,
                    signed,
                },
            );
        }
    }
    if !matched {
        push(Level::Error, Finding::KeyMatchesNothing { object });
    }
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
