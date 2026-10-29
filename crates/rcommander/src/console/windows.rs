//! `AttachConsole(ATTACH_PARENT_PROCESS)`, the one Win32 call of [`super::attach_parent`].

use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

#[allow(
    unsafe_code,
    reason = "AttachConsole is a plain Win32 call: it takes a process id, reads and writes no \
              memory of ours, and fails harmlessly when there is no parent console or this \
              process has one already; windows-sys offers it only as an unsafe extern fn"
)]
pub(super) fn attach_parent() {
    // SAFETY: no pointers, no handles of ours; the result only says whether a console is
    // attached now, and without one the output goes nowhere, as before.
    let _attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
