//! Instants as text (RC-110-36): RFC 3339 in UTC, the fraction of a second in none, 3, 6 or 9
//! digits - the text chrono wrote (`SecondsFormat::AutoSi`) before jiff took its place, and the
//! one the published manifests, `update.state`, the journal and `update.status` keep. Reading
//! takes any RFC 3339 offset, as it did.

use jiff::{Timestamp, tz::Offset, tz::TimeZone};

/// The digits of the fraction: none, milli-, micro- or nanoseconds, the fewest that keep it.
fn digits(at: Timestamp) -> usize {
    match at.subsec_nanosecond() {
        0 => 0,
        nanos if nanos % 1_000_000 == 0 => 3,
        nanos if nanos % 1_000 == 0 => 6,
        _ => 9,
    }
}

/// `2025-11-19T10:00:00Z`, `2025-11-19T10:00:00.250Z`: what the stored formats hold.
#[must_use]
pub fn rfc3339(at: Timestamp) -> String {
    let precision = digits(at);
    format!("{at:.precision$}")
}

/// `2025-11-19T10:00:00+00:00`: the same with a numeric offset, as `rc-pack` prints it.
#[must_use]
pub fn rfc3339_offset(at: Timestamp) -> String {
    let precision = digits(at);
    format!("{:.precision$}", at.display_with_offset(Offset::UTC))
}

/// `2025-11-19 10:00:00 UTC`: an instant in an error's text.
#[must_use]
pub fn utc_text(at: Timestamp) -> String {
    let time = TimeZone::UTC.to_datetime(at);
    let precision = digits(at);
    format!("{} {:.precision$} UTC", time.date(), time.time())
}

/// An instant in [`rfc3339`]'s text, for `#[serde(with = "rc_update::time::serde_rfc3339")]`.
pub mod serde_rfc3339 {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    /// Writes [`super::rfc3339`].
    ///
    /// # Errors
    ///
    /// The serializer's.
    pub fn serialize<S: Serializer>(at: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::rfc3339(*at))
    }

    /// Reads RFC 3339 with any offset.
    ///
    /// # Errors
    ///
    /// No string, or no instant.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }

    /// The same for an optional instant: `null` for none.
    pub mod option {
        use jiff::Timestamp;
        use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

        /// Writes [`super::super::rfc3339`], or `null`.
        ///
        /// # Errors
        ///
        /// The serializer's.
        pub fn serialize<S: Serializer>(
            at: &Option<Timestamp>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match at {
                Some(at) => serializer.serialize_some(&super::super::rfc3339(*at)),
                None => serializer.serialize_none(),
            }
        }

        /// Reads RFC 3339 with any offset, or `null`.
        ///
        /// # Errors
        ///
        /// No string, or no instant.
        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Timestamp>, D::Error> {
            Option::<String>::deserialize(deserializer)?
                .map(|text| text.parse().map_err(D::Error::custom))
                .transpose()
        }
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
