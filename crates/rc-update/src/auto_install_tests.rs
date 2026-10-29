use super::*;

fn at(minutes: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_790_000_000, 0).expect("timestamp") + TimeDelta::minutes(minutes)
}

fn ready(mode: AutoMode) -> Moment<'static> {
    Moment {
        mode,
        installs_itself: true,
        ready: Some("2.0.0"),
        last_input: Some(at(0)),
        ..Moment::default()
    }
}

const IDLE: TimeDelta = TimeDelta::minutes(30);

#[test]
fn off_never_installs() {
    let moment = ready(AutoMode::Off);
    assert_eq!(on_quit(&moment), Err(Wait::Off));
    let mut clock = IdleClock::default();
    for minute in [0, 60, 600] {
        assert_eq!(clock.observe(&moment, IDLE, at(minute)), Err(Wait::Off));
    }
}

#[test]
fn on_quit_installs_only_at_quitting() {
    let moment = ready(AutoMode::OnQuit);
    assert_eq!(on_quit(&moment), Ok(()));
    let mut clock = IdleClock::default();
    for minute in [0, 60, 600] {
        assert_eq!(
            clock.observe(&moment, IDLE, at(minute)),
            Err(Wait::UntilQuit)
        );
    }
    // Quitting with jobs in the queue asks as always and ends without the update.
    let busy = Moment {
        jobs: true,
        ..moment
    };
    assert_eq!(on_quit(&busy), Err(Wait::Jobs));
}

#[test]
fn when_idle_restarts_after_the_quiet_period() {
    let moment = ready(AutoMode::WhenIdle);
    let mut clock = IdleClock::default();
    assert_eq!(clock.observe(&moment, IDLE, at(10)), Err(Wait::Input));
    assert_eq!(clock.observe(&moment, IDLE, at(31)), Err(Wait::Settling));
    assert_eq!(clock.observe(&moment, IDLE, at(61)), Ok(()));
    assert_eq!(
        on_quit(&moment),
        Ok(()),
        "when_idle installs at quitting too"
    );
}

/// A job, a terminal or SSH session, a dialog or input each wait and start the quiet anew.
#[test]
fn every_breaking_condition_starts_the_quiet_period_again() {
    let base = ready(AutoMode::WhenIdle);
    let breaking = [
        (Moment { jobs: true, ..base }, Wait::Jobs),
        (
            Moment {
                sessions: true,
                ..base
            },
            Wait::Sessions,
        ),
        (
            Moment {
                dialog: true,
                ..base
            },
            Wait::Dialog,
        ),
        (
            Moment {
                last_input: Some(at(100)),
                ..base
            },
            Wait::Input,
        ),
    ];
    for (moment, wait) in breaking {
        let mut clock = IdleClock::default();
        assert_eq!(clock.observe(&base, IDLE, at(40)), Err(Wait::Settling));
        assert_eq!(clock.observe(&moment, IDLE, at(101)), Err(wait));
        // Quiet again: the period counts from now, not from the first observation.
        assert_eq!(clock.observe(&base, IDLE, at(102)), Err(Wait::Settling));
        assert_eq!(clock.observe(&base, IDLE, at(131)), Err(Wait::Settling));
        assert_eq!(clock.observe(&base, IDLE, at(133)), Ok(()));
    }
}

#[test]
fn a_failed_version_or_an_installation_that_does_not_update_itself_never_goes_by_itself() {
    let failed = Moment {
        failed_before: true,
        ..ready(AutoMode::WhenIdle)
    };
    assert_eq!(on_quit(&failed), Err(Wait::FailedBefore));
    let unknown = Moment {
        installs_itself: false,
        ..ready(AutoMode::WhenIdle)
    };
    assert_eq!(on_quit(&unknown), Err(Wait::Unsupported));
    let mut clock = IdleClock::default();
    assert_eq!(
        clock.observe(&unknown, IDLE, at(600)),
        Err(Wait::Unsupported)
    );
    let nothing = Moment {
        ready: None,
        ..ready(AutoMode::OnQuit)
    };
    assert_eq!(on_quit(&nothing), Err(Wait::NothingReady));
}

#[test]
fn an_unknown_setting_reads_as_off() {
    assert_eq!(AutoMode::parse("whenever"), AutoMode::Off);
    assert_eq!(AutoMode::parse("on_quit"), AutoMode::OnQuit);
    assert_eq!(AutoMode::parse("when_idle").as_str(), "when_idle");
}
