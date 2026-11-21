//! The times of a rename as text (RC-110-36): a local time `2026-11-07T08:15:00`, an instant
//! `2026-11-07T07:15:00Z`, the fraction of a second in none, 3, 6 or 9 digits - the text chrono
//! wrote before jiff took its place, which MCP calls and the undo log keep. Reading takes what
//! jiff reads: the same, a space for the `T`, a time without seconds.

use jiff::{Timestamp, civil::DateTime};

/// The digits of a fraction of `nanos`: none, milli-, micro- or nanoseconds.
fn digits(nanos: i32) -> usize {
    match nanos {
        0 => 0,
        nanos if nanos % 1_000_000 == 0 => 3,
        nanos if nanos % 1_000 == 0 => 6,
        _ => 9,
    }
}

/// `2026-11-07T08:15:00`.
pub(crate) fn local_text(time: DateTime) -> String {
    let precision = digits(time.subsec_nanosecond());
    format!("{time:.precision$}")
}

/// `2026-11-07T07:15:00Z`.
pub(crate) fn instant_text(at: Timestamp) -> String {
    let precision = digits(at.subsec_nanosecond());
    format!("{at:.precision$}")
}

/// An optional local time: [`local_text`] or `null`.
pub(crate) mod local_option {
    use jiff::civil::DateTime;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(crate) fn serialize<S: Serializer>(
        time: &Option<DateTime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match time {
            Some(time) => serializer.serialize_some(&super::local_text(*time)),
            None => serializer.serialize_none(),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<DateTime>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| text.parse().map_err(D::Error::custom))
            .transpose()
    }
}

/// An instant: [`instant_text`]; any RFC 3339 offset reads.
pub(crate) mod instant {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(crate) fn serialize<S: Serializer>(
        at: &Timestamp,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::instant_text(*at))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Timestamp, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

#[cfg(test)]
#[path = "serde_time_tests.rs"]
mod tests;
