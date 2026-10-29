use std::time::{Duration, SystemTime};

use rc_engine::{
    Details, Entry, EntryKind, ItemProperties, PropertiesReport, PropertyCapabilities, VfsPath,
};
use rc_vfs::{Attributes, WindowsFlags};

use super::{AttributeForm, Flag, FormMsg, time_text};

fn report(mode: Option<u32>) -> PropertiesReport {
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    PropertiesReport {
        items: vec![ItemProperties {
            path: VfsPath::local("/data/a.txt"),
            entry: Entry {
                name: "a.txt".into(),
                kind: EntryKind::File,
                size: 3,
                modified: Some(modified),
                created: None,
                attributes: Attributes {
                    mode,
                    readonly: false,
                    windows: mode.is_none().then_some(WindowsFlags {
                        hidden: false,
                        system: false,
                        archive: true,
                    }),
                },
                hidden: false,
                owner: None,
            },
            details: Details::default(),
            usage: None,
            comment: None,
        }],
        total: None,
        capabilities: PropertyCapabilities::NONE,
    }
}

#[test]
fn an_untouched_form_changes_nothing() {
    let form = AttributeForm::from_report(&report(Some(0o644)));
    assert_eq!(form.mode, "644");
    assert!(form.has_mode && !form.has_dirs);
    assert!(form.change().expect("reads").is_empty());
}

#[test]
fn only_what_was_touched_is_sent() {
    let mut form = AttributeForm::from_report(&report(None));
    assert!(form.flag(Flag::Archive));
    form.update(FormMsg::Flag(Flag::Archive, false));
    form.update(FormMsg::Flag(Flag::Hidden, true));
    let change = form.change().expect("reads");
    assert_eq!(change.archive, Some(false));
    assert_eq!(change.hidden, Some(true));
    assert_eq!(
        (change.readonly, change.system, change.mode),
        (None, None, None)
    );
}

#[test]
fn mode_and_times_are_read_or_refused() {
    let mut form = AttributeForm::from_report(&report(Some(0o644)));
    form.update(FormMsg::Mode("750".into()));
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    form.update(FormMsg::Modified(time_text(Some(then))));
    let change = form.change().expect("reads");
    assert_eq!(change.mode, Some(0o750));
    assert_eq!(change.modified, Some(then));
    form.update(FormMsg::Mode("9".into()));
    assert!(form.change().is_err());
    form.update(FormMsg::Mode("750".into()));
    form.update(FormMsg::Modified("yesterday".into()));
    assert!(form.change().is_err());
}

#[test]
fn now_sets_both_times() {
    let mut form = AttributeForm::from_report(&report(None));
    form.update(FormMsg::Now);
    let change = form.change().expect("reads");
    assert!(change.modified.is_some() && change.created.is_some());
}
