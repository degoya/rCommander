//! Windows: a terminal's program in a job object of its own (RC-040-06), so that it and what it
//! started end with the terminal - and with rCommander, however it ends: the job is made with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, and Windows closes its handle when rCommander's process
//! ends, after a crash or from the task manager too. Before, a shell outlived a rCommander that
//! did not drop its sessions, and a program a shell had started (`ssh`, `wsl`, a build) outlived
//! the shell, which was ended alone.
//!
//! With `whole_tree` off (the setting `terminal.end_programs_on_close`) the job also has
//! `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`: what the program starts is outside the job, only the
//! program itself ends with it.
//!
//! The program is assigned right after `portable-pty` started it - `portable-pty` cannot start
//! it suspended -, so a child it starts in that instant stays outside; a shell takes far longer
//! to start anything. It is assigned by the process handle `portable-pty` holds (RC-080-51),
//! never by its id: a program that ended at once leaves its id free for another process, while
//! the handle names this process until it is closed.
//!
//! A step Windows refuses is a `warn` with the step and Windows' reason, never the program's
//! command line (RC-080-58); the program then runs without a job.
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers the job functions as
//! bare `extern` functions, and neither std nor `winsafe` has them. Every call has its `SAFETY:`
//! note.
#![allow(
    unsafe_code,
    reason = "the job object functions are bare extern fns in windows-sys, std and winsafe have \
              none; every block has its SAFETY note"
)]

use std::{mem::size_of, ptr::null};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    },
};

/// The exit code of the processes the job ends.
const ENDED: u32 = 1;

/// A job object with one program in it; closing it ends what is in it.
pub(super) struct Job(HANDLE);

// SAFETY: a job object's handle is a kernel handle, valid from any thread; the job functions
// called on it are thread-safe, and only `Drop` closes it.
unsafe impl Send for Job {}
// SAFETY: as above; `kill` takes `&self` and calls a thread-safe kernel function.
unsafe impl Sync for Job {}

impl Job {
    /// A new job with the program `child` started in it, by its process handle; `None` when it
    /// has none or Windows refuses one of the steps (the program is then ended alone).
    pub(super) fn of(
        child: &(dyn portable_pty::Child + Send + Sync),
        whole_tree: bool,
    ) -> Option<Self> {
        // The handle stays `child`'s: it is open while `child` lives, which outlasts this call.
        let Some(process) = child.as_raw_handle() else {
            tracing::warn!(
                step = "process handle",
                "the terminal's program runs without a job object"
            );
            return None;
        };
        Self::around(process, whole_tree)
    }

    /// A new job with the process of the open handle `process` in it; `None` when Windows
    /// refuses one of the steps. The handle stays the caller's.
    pub(super) fn around(process: HANDLE, whole_tree: bool) -> Option<Self> {
        // SAFETY: no security attributes and no name: a new anonymous job, or null.
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return refused("CreateJobObjectW");
        }
        // Owned from here: dropped, it closes the handle.
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = if whole_tree {
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        } else {
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK
        };
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .expect("the limit structure is a few hundred bytes");
        // SAFETY: `job.0` is the job just created; `limits` is the structure the class names,
        // alive for the call, and `size` its size.
        let limited = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size,
            )
        };
        if limited == 0 {
            return refused("SetInformationJobObject");
        }
        // SAFETY: both handles are open: the job's, and the process's, which the caller keeps
        // open for the call; a handle from `CreateProcessW` (as `portable-pty`'s and std's are)
        // has every right, `PROCESS_SET_QUOTA | PROCESS_TERMINATE` among them.
        let assigned = unsafe { AssignProcessToJobObject(job.0, process) };
        if assigned == 0 {
            return refused("AssignProcessToJobObject");
        }
        Some(job)
    }

    /// Ends every process in the job.
    pub(super) fn kill(&self) {
        // SAFETY: `self.0` stays open until `Drop`; a job whose processes ended already is no
        // error worth handling.
        unsafe { TerminateJobObject(self.0, ENDED) };
    }
}

/// Logs the step Windows refused with its last error - read before anything else calls
/// Windows, the job's `CloseHandle` too - and answers `None`.
fn refused(step: &'static str) -> Option<Job> {
    let error = std::io::Error::last_os_error();
    tracing::warn!(
        step,
        code = error.raw_os_error(),
        %error,
        "the terminal's program runs without a job object"
    );
    None
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle `around` created, closed once; with `KILL_ON_JOB_CLOSE` this ends
        // what is still in the job.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
