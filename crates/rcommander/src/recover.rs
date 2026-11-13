//! What a panic leaves behind (RC-080-71): the part of `rc-recover` this crate uses, under the
//! path every call site names (`crate::recover::...`). The code and its tests are in
//! `crates/rc-recover` (RC-110-30).

pub(crate) use rc_recover::{LockOrRecover, LogPanic, RwLockOrRecover};
