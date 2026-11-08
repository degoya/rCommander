//! A small DER reader and writer for the key formats x509-parser does not read (PKCS#1, PKCS#8,
//! SEC1, DSA, SubjectPublicKeyInfo) and for the canonical SubjectPublicKeyInfo every public key
//! is written as. Input from outside: every read is bounds-checked and answers `None` on
//! anything it does not expect; nothing here panics or recurses.

/// The universal tags read and written here.
pub mod tag {
    pub const INTEGER: u8 = 0x02;
    pub const BIT_STRING: u8 = 0x03;
    pub const OCTET_STRING: u8 = 0x04;
    pub const NULL: u8 = 0x05;
    pub const OID: u8 = 0x06;
    pub const SEQUENCE: u8 = 0x30;
}

/// One element: its tag byte and its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tlv<'a> {
    pub tag: u8,
    pub content: &'a [u8],
}

/// Reads one element from the front of `input`: low tag numbers only, definite lengths of at
/// most four length bytes. Returns the element and what follows it.
#[must_use]
pub fn read(input: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    let (&tag, rest) = input.split_first()?;
    if tag & 0x1f == 0x1f {
        return None;
    }
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let mut length = 0usize;
        for &byte in rest.get(..count)? {
            length = length.checked_mul(256)?.checked_add(usize::from(byte))?;
        }
        (length, rest.get(count..)?)
    };
    let content = rest.get(..length)?;
    Some((Tlv { tag, content }, rest.get(length..)?))
}

/// The content of the SEQUENCE at the front of `input`, and what follows it.
#[must_use]
pub fn sequence(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let (tlv, rest) = read(input)?;
    (tlv.tag == tag::SEQUENCE).then_some((tlv.content, rest))
}

/// The elements of a constructed value, front to back.
#[derive(Clone, Copy, Debug)]
pub struct Elements<'a> {
    rest: &'a [u8],
}

impl<'a> Elements<'a> {
    #[must_use]
    pub fn new(content: &'a [u8]) -> Self {
        Self { rest: content }
    }

    /// The next element's content when it has `tag`; `None` (and nothing taken) otherwise.
    pub fn pop(&mut self, tag: u8) -> Option<&'a [u8]> {
        let (tlv, rest) = read(self.rest)?;
        if tlv.tag != tag {
            return None;
        }
        self.rest = rest;
        Some(tlv.content)
    }

    /// The tag of the next element, if any.
    #[must_use]
    pub fn peek_tag(&self) -> Option<u8> {
        self.rest.first().copied()
    }
}

impl<'a> Iterator for Elements<'a> {
    type Item = Tlv<'a>;

    fn next(&mut self) -> Option<Tlv<'a>> {
        let (tlv, rest) = read(self.rest)?;
        self.rest = rest;
        Some(tlv)
    }
}

/// An INTEGER's content without its leading zero bytes: the magnitude of a non-negative number.
#[must_use]
pub fn magnitude(content: &[u8]) -> &[u8] {
    let start = content
        .iter()
        .position(|&byte| byte != 0)
        .unwrap_or(content.len());
    content.get(start..).unwrap_or_default()
}

/// A small non-negative INTEGER (a version), `None` past `u64` or when negative.
#[must_use]
pub fn small_uint(content: &[u8]) -> Option<u64> {
    if content.first().is_some_and(|&byte| byte & 0x80 != 0) {
        return None;
    }
    let digits = magnitude(content);
    if digits.len() > 8 {
        return None;
    }
    Some(
        digits
            .iter()
            .fold(0u64, |value, &byte| (value << 8) | u64::from(byte)),
    )
}

/// The number of significant bits of a magnitude.
#[must_use]
pub fn bit_length(magnitude: &[u8]) -> u32 {
    let digits = self::magnitude(magnitude);
    match digits.first() {
        None => 0,
        Some(&top) => {
            let bytes = u32::try_from(digits.len()).unwrap_or(u32::MAX / 8);
            bytes.saturating_mul(8) - top.leading_zeros()
        }
    }
}

/// A BIT STRING's bits when no bit is unused (every key here is whole bytes).
#[must_use]
pub fn whole_bits(content: &[u8]) -> Option<&[u8]> {
    match content.split_first() {
        Some((0, bits)) => Some(bits),
        _ => None,
    }
}

/// Appends one element of `tag` around `content`.
pub fn write(tag: u8, content: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    let length = content.len();
    if length < 0x80 {
        // Below 0x80, so the cast keeps every bit.
        out.push(u8::try_from(length).unwrap_or(0x7f));
    } else {
        let bytes = length.to_be_bytes();
        let skip = bytes.iter().take_while(|&&byte| byte == 0).count();
        let significant = bytes.get(skip..).unwrap_or_default();
        out.push(0x80 | u8::try_from(significant.len()).unwrap_or(0));
        out.extend_from_slice(significant);
    }
    out.extend_from_slice(content);
}

/// One element of `tag` around `content`, as a value of its own.
#[must_use]
pub fn element(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + 6);
    write(tag, content, &mut out);
    out
}

/// A non-negative INTEGER of `magnitude`, in its shortest form.
#[must_use]
pub fn integer(magnitude: &[u8]) -> Vec<u8> {
    let digits = self::magnitude(magnitude);
    let mut content = Vec::with_capacity(digits.len() + 1);
    if digits.first().is_none_or(|&top| top & 0x80 != 0) {
        content.push(0);
    }
    content.extend_from_slice(digits);
    element(tag::INTEGER, &content)
}

/// A BIT STRING of whole bytes.
#[must_use]
pub fn bit_string(bits: &[u8]) -> Vec<u8> {
    let mut content = Vec::with_capacity(bits.len() + 1);
    content.push(0);
    content.extend_from_slice(bits);
    element(tag::BIT_STRING, &content)
}

#[cfg(test)]
#[path = "der_tests.rs"]
mod tests;
