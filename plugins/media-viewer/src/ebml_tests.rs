use super::*;
use crate::types::FailureKind;

#[test]
fn vints_read_their_length_and_value() {
    assert_eq!(vint(&[0x81]).expect("one byte"), (1, Some(1)));
    assert_eq!(vint(&[0x40, 0x02]).expect("two bytes"), (2, Some(2)));
    assert_eq!(
        vint(&[0x01, 0, 0, 0, 0, 0, 0x01, 0x00]).expect("eight bytes"),
        (8, Some(256))
    );
}

#[test]
fn all_ones_is_an_unknown_size() {
    assert_eq!(vint(&[0xFF]).expect("one byte"), (1, None));
    assert_eq!(
        vint(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]).expect("eight bytes"),
        (8, None)
    );
}

#[test]
fn a_vint_longer_than_eight_bytes_or_cut_off_is_bad_data() {
    for bytes in [&[0x00, 0xFF][..], &[0x40][..], &[0x01, 0xFF][..], &[][..]] {
        let failure = vint(bytes).expect_err("refused");
        assert_eq!(failure.kind, FailureKind::BadData);
    }
}

#[test]
fn element_ids_keep_their_marker() {
    assert_eq!(
        element_id(&[0x1A, 0x45, 0xDF, 0xA3]).expect("EBML"),
        (4, 0x1A45_DFA3)
    );
    assert_eq!(element_id(&[0xAE]).expect("TrackEntry"), (1, 0xAE));
}

#[test]
fn an_id_of_all_ones_or_over_four_bytes_is_bad_data() {
    for bytes in [
        &[0xFF][..],
        &[0x08, 0, 0, 0, 1][..],
        &[0x1F, 0xFF, 0xFF, 0xFF][..],
    ] {
        let failure = element_id(bytes).expect_err("refused");
        assert_eq!(failure.kind, FailureKind::BadData);
    }
}
