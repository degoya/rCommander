use super::*;

#[test]
fn reads_short_and_long_lengths() {
    let short = [0x02, 0x01, 0x05, 0xff];
    let (tlv, rest) = read(&short).expect("short");
    assert_eq!(
        tlv,
        Tlv {
            tag: tag::INTEGER,
            content: &[0x05]
        }
    );
    assert_eq!(rest, &[0xff]);

    let mut long = vec![0x04, 0x82, 0x01, 0x00];
    long.extend(std::iter::repeat_n(7u8, 256));
    let (tlv, rest) = read(&long).expect("long");
    assert_eq!(tlv.content.len(), 256);
    assert!(rest.is_empty());
}

#[test]
fn refuses_what_it_does_not_expect() {
    assert_eq!(read(&[]), None);
    assert_eq!(read(&[0x30]), None);
    // High tag number, indefinite length, five length bytes, content past the end.
    assert_eq!(read(&[0x1f, 0x01, 0x00]), None);
    assert_eq!(read(&[0x30, 0x80, 0x00, 0x00]), None);
    assert_eq!(read(&[0x30, 0x85, 1, 0, 0, 0, 0]), None);
    assert_eq!(read(&[0x30, 0x03, 0x02, 0x01]), None);
}

#[test]
fn elements_take_only_the_tag_asked_for() {
    let content = [0x02, 0x01, 0x01, 0x04, 0x00];
    let mut elements = Elements::new(&content);
    assert_eq!(elements.pop(tag::OCTET_STRING), None);
    assert_eq!(elements.pop(tag::INTEGER), Some(&[1u8][..]));
    assert_eq!(elements.peek_tag(), Some(tag::OCTET_STRING));
    assert_eq!(elements.pop(tag::OCTET_STRING), Some(&[][..]));
    assert_eq!(elements.next(), None);
}

#[test]
fn numbers_lose_their_leading_zeros_and_keep_their_size() {
    assert_eq!(magnitude(&[0, 0, 0x80, 1]), &[0x80, 1]);
    assert_eq!(magnitude(&[0, 0]), &[] as &[u8]);
    assert_eq!(bit_length(&[0x00, 0x80, 0x00]), 16);
    assert_eq!(bit_length(&[0x01]), 1);
    assert_eq!(bit_length(&[]), 0);
    assert_eq!(small_uint(&[0x00, 0x03]), Some(3));
    assert_eq!(small_uint(&[0x80]), None);
    assert_eq!(small_uint(&[1, 2, 3, 4, 5, 6, 7, 8, 9]), None);
}

#[test]
fn writes_shortest_forms() {
    assert_eq!(integer(&[0x7f]), vec![0x02, 0x01, 0x7f]);
    assert_eq!(integer(&[0x00, 0x80]), vec![0x02, 0x02, 0x00, 0x80]);
    assert_eq!(integer(&[]), vec![0x02, 0x01, 0x00]);
    assert_eq!(bit_string(&[0xaa]), vec![0x03, 0x02, 0x00, 0xaa]);
    let long = element(tag::OCTET_STRING, &[0u8; 300]);
    assert_eq!(long.get(..4), Some(&[0x04, 0x82, 0x01, 0x2c][..]));
    let (tlv, rest) = read(&long).expect("reads back");
    assert_eq!(tlv.content.len(), 300);
    assert!(rest.is_empty());
}

#[test]
fn whole_bits_refuses_unused_bits() {
    assert_eq!(whole_bits(&[0, 1, 2]), Some(&[1u8, 2][..]));
    assert_eq!(whole_bits(&[3, 1, 2]), None);
    assert_eq!(whole_bits(&[]), None);
}
