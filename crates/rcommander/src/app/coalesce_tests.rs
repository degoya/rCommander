//! Cursor steps that piled up in the job queue reach the engine as one step; every other job
//! keeps its place.

use std::collections::VecDeque;

use rc_engine::{Command, CursorStep, PanelId};
use tokio::sync::mpsc;

use super::{next, push};
use crate::app::Job;

fn step(panel: PanelId, step: CursorStep) -> Job {
    Job {
        panel,
        commands: vec![Command::MoveCursor { panel: None, step }],
    }
}

fn steps(jobs: &VecDeque<Job>) -> Vec<(PanelId, Vec<Command>)> {
    jobs.iter()
        .map(|job| (job.panel, job.commands.clone()))
        .collect()
}

#[tokio::test]
async fn fifty_piled_up_arrow_steps_become_one_step() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    for _ in 0..50 {
        sender
            .send(step(PanelId::Left, CursorStep::Down))
            .expect("open");
    }
    let mut waiting = VecDeque::new();
    let first = next(&mut receiver, &mut waiting).await.expect("a job");
    assert_eq!(
        first.commands,
        [Command::MoveCursor {
            panel: None,
            step: CursorStep::Down
        }]
    );
    assert!(waiting.is_empty(), "the other 49 were the same step");
    drop(sender);
    assert!(next(&mut receiver, &mut waiting).await.is_none());
}

#[tokio::test]
async fn jobs_that_came_one_at_a_time_are_each_run() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let mut waiting = VecDeque::new();
    for _ in 0..3 {
        sender
            .send(step(PanelId::Left, CursorStep::Down))
            .expect("open");
        assert!(next(&mut receiver, &mut waiting).await.is_some());
    }
    assert!(waiting.is_empty());
}

#[test]
fn only_a_run_of_one_step_for_one_panel_is_merged() {
    let enter = Job {
        panel: PanelId::Left,
        commands: vec![Command::OpenEntry { panel: None }],
    };
    let two = Job {
        panel: PanelId::Left,
        commands: vec![
            Command::MoveCursor {
                panel: None,
                step: CursorStep::Down,
            },
            Command::MoveCursor {
                panel: None,
                step: CursorStep::Down,
            },
        ],
    };
    let mut waiting = VecDeque::new();
    for job in [
        step(PanelId::Left, CursorStep::Down),
        step(PanelId::Left, CursorStep::Down),
        step(PanelId::Left, CursorStep::Up),
        step(PanelId::Right, CursorStep::Up),
        step(PanelId::Right, CursorStep::Up),
        enter,
        step(PanelId::Right, CursorStep::Up),
        two,
        step(PanelId::Left, CursorStep::Down),
    ] {
        push(&mut waiting, job);
    }
    let down = |panel| {
        (
            panel,
            vec![Command::MoveCursor {
                panel: None,
                step: CursorStep::Down,
            }],
        )
    };
    let up = |panel| {
        (
            panel,
            vec![Command::MoveCursor {
                panel: None,
                step: CursorStep::Up,
            }],
        )
    };
    assert_eq!(
        steps(&waiting),
        [
            down(PanelId::Left),
            up(PanelId::Left),
            up(PanelId::Right),
            (PanelId::Left, vec![Command::OpenEntry { panel: None }]),
            up(PanelId::Right),
            (
                PanelId::Left,
                vec![
                    Command::MoveCursor {
                        panel: None,
                        step: CursorStep::Down,
                    };
                    2
                ]
            ),
            down(PanelId::Left),
        ]
    );
}
