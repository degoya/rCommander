//! The owner of a listed entry (RC-060-02). A version 3 server sends a `longname` with each
//! name, `ls -l` style (`-rw-r--r--  1 alice  staff  1234 Jan  1 12:00 a.txt` from OpenSSH), and
//! its third and fourth fields are the only place user and group appear by name; the attributes
//! carry the numbers. Both are a server's word: a name is believed only when it is short and
//! plain, and a `longname` of another shape names no owner.

use rc_vfs::Owner;
use russh_sftp::protocol::FileAttributes;

/// The longest user or group name believed.
const MAX_NAME: usize = 64;

/// The owner the attributes name: names where the server sent them, else the numbers; `None`
/// when they carry no owner.
pub(crate) fn of_attrs(attrs: &FileAttributes) -> Option<Owner> {
    let user = attrs.user.clone().filter(|name| plain(name));
    let group = attrs.group.clone().filter(|name| plain(name));
    Some(Owner {
        user: user.or_else(|| attrs.uid.map(|uid| uid.to_string()))?,
        group: group.or_else(|| attrs.gid.map(|gid| gid.to_string()))?,
    })
}

/// The owner a `longname` names: the two fields after a mode of ten or more characters and a
/// link count, followed by at least the size; `None` for any other shape.
pub(crate) fn of_longname(longname: &str) -> Option<Owner> {
    let mut fields = longname.split_ascii_whitespace();
    let mode = fields.next()?;
    let links = fields.next()?;
    let (user, group) = (fields.next()?, fields.next()?);
    let shaped = mode.len() >= 10
        && mode.is_ascii()
        && links.bytes().all(|byte| byte.is_ascii_digit())
        && fields.next().is_some();
    (shaped && plain(user) && plain(group)).then(|| Owner {
        user: user.to_owned(),
        group: group.to_owned(),
    })
}

/// Not empty, at most [`MAX_NAME`] characters, no control character and no space.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(|c| c.is_control() || c.is_whitespace())
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
