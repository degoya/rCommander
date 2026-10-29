use std::{
    os::windows::io::AsRawHandle as _,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use rc_process::NoConsoleWindow as _;

use super::Job;

/// A `cmd` that waits half a minute in a `ping` it started: a program with a child.
fn waiting_shell() -> Child {
    Command::new("cmd")
        .args(["/c", "ping -n 30 127.0.0.1 >NUL"])
        .no_console_window()
        .spawn()
        .expect("cmd starts")
}

/// Whether `child` has ended within a few seconds.
fn ends(child: &mut Child) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if child.try_wait().expect("the child can be asked").is_some() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

/// `kill` ends the program in the job.
#[test]
fn kill_ends_the_program() {
    let mut shell = waiting_shell();
    let job = Job::around(shell.as_raw_handle(), true).expect("a job");
    job.kill();
    assert!(ends(&mut shell));
}

/// Closing the job - as Windows does when rCommander's process ends - ends the program too.
#[test]
fn closing_the_job_ends_the_program() {
    let mut shell = waiting_shell();
    let job = Job::around(shell.as_raw_handle(), false).expect("a job");
    drop(job);
    assert!(ends(&mut shell));
}

/// A step Windows refuses answers `None` - logged, the job's handle closed - and ends nothing.
#[test]
fn a_refused_step_is_no_job() {
    assert!(Job::around(std::ptr::null_mut(), true).is_none());
}
