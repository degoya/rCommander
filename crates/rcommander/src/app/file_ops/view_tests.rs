use super::*;

#[test]
fn the_time_left_reads_as_a_clock() {
    assert_eq!(duration(0), "0:00");
    assert_eq!(duration(309), "5:09");
    assert_eq!(duration(3909), "1:05:09");
}

#[test]
fn every_preset_has_its_own_label() {
    let labels: Vec<&str> = Preset::ALL.into_iter().map(preset_label).collect();
    let mut unique = labels.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), labels.len());
}

/// RC-080-53: packing and unpacking say so in their title, as the queue says it.
#[test]
fn packing_and_unpacking_have_their_own_title() {
    let path = rc_engine::VfsPath::local("/data/a.zip");
    let pack = JobKind::Pack {
        sources: vec![path.clone()],
        target: path.clone(),
        settings: Default::default(),
    };
    let extract = JobKind::Extract {
        archives: vec![path.clone()],
        target: path,
        settings: Default::default(),
    };
    assert_eq!(kind_title(&pack), t!("ops.title.pack"));
    assert_eq!(kind_title(&extract), t!("ops.title.extract"));
    assert_ne!(kind_title(&pack), t!("ops.title.other"));
}
