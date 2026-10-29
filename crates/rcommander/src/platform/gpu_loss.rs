//! The question after a lost graphics device (`crate::gpu_loss`): the system's own message box,
//! which draws without the app's surface (`windows/gpu_loss.rs`). Elsewhere there is none for
//! now, and the window stays as it is.

/// Asks whether to restart rCommander and waits for the answer; `false` without a message box.
pub fn ask_restart(title: &str, text: &str) -> bool {
    #[cfg(windows)]
    {
        super::windows::gpu_loss::ask_restart(title, text)
    }
    #[cfg(not(windows))]
    {
        let _ = (title, text);
        false
    }
}
