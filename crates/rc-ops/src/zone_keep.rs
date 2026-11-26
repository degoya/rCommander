//! A copy's, a move's and an unpacking's side of the Mark-of-the-Web (RC-110-56, `zone.rs`):
//! each file written onto this machine's disks gets the zone its source carries, before its
//! times and attributes are set - a read-only file takes no stream, and the stream's write would
//! move its modification time. A zone that cannot be written fails nothing: the job lists it
//! once (`zone_not_kept`), the log has the first.

use std::{io, path::Path};

use rc_vfs::{BackendId, VfsPath};

use crate::{
    Failure, OpsError,
    recover::LogPanic as _,
    runner::Runner,
    walk::Item,
    zone::{self, Carry},
};

/// What a job knows of zones.
#[derive(Default)]
pub(crate) struct ZoneState {
    /// The target is this machine's disks, the only place a zone is written.
    pub(crate) onto_local: bool,
    /// What the files of one source backend carry on, found once for it.
    carry: Option<(BackendId, Carry)>,
    /// A zone that could not be written was listed; the later ones are not.
    listed: bool,
}

impl ZoneState {
    /// For a job writing into `target`.
    pub(crate) fn onto(&mut self, target: &BackendId) {
        self.onto_local = *target == BackendId::Local;
    }
}

impl Runner {
    /// Gives `dest`, the copy of `item`, the zone its source carries.
    pub(crate) async fn keep_zone(&mut self, item: &Item, dest: &Path) {
        if !zone::SUPPORTED || !self.zone.onto_local {
            return;
        }
        let carry = self.zone_carry().await;
        if carry == Carry::Nothing {
            return;
        }
        let (source, target) = (item.path.clone(), dest.to_path_buf());
        let kept = tokio::task::spawn_blocking(move || zone::keep(&carry, &source, &target))
            .await
            .log_panic("ops.zone")
            .map_err(io::Error::other)
            .and_then(|kept| kept);
        if let Err(error) = kept {
            self.zone_not_kept(item, dest, &error);
        }
    }

    /// What the files of the job's source carry on, found once per source backend: an
    /// unpacking of several archives has one each.
    async fn zone_carry(&mut self) -> Carry {
        if let Some((backend, carry)) = &self.zone.carry
            && *backend == self.src_id
        {
            return carry.clone();
        }
        let origin = zone::origin(&self.src_id);
        let host = match (&origin, &self.reconnect) {
            (zone::Origin::Server(server), Some(hook)) => hook.host_url(server).await,
            _ => None,
        };
        let carry = tokio::task::spawn_blocking(move || zone::carry(&origin, host.as_deref()))
            .await
            .log_panic("ops.zone")
            .unwrap_or(Carry::Nothing);
        self.zone.carry = Some((self.src_id.clone(), carry.clone()));
        carry
    }

    /// Lists the first file of the job whose zone could not be written; the copy stands.
    fn zone_not_kept(&mut self, item: &Item, dest: &Path, error: &io::Error) {
        if self.zone.listed {
            tracing::debug!(%error, "another copy without its zone");
            return;
        }
        self.zone.listed = true;
        tracing::warn!(%error, "a copy did not keep its mark that it came from the internet");
        let mut failure = Failure::new(
            &self.src_path(&item.path),
            &OpsError::ZoneNotKept {
                path: dest.to_path_buf(),
            },
        );
        failure.target = Some(Box::new(VfsPath::local(dest)));
        self.job.update(|snapshot| snapshot.failures.push(failure));
    }
}
