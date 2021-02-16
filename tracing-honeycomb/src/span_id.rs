use std::fmt::{self, Display};
use std::str::FromStr;
/// Unique Span identifier.
///
/// Combines a span's `tracing::Id` with an instance identifier to avoid id collisions in distributed scenarios.
///
/// `Display` and `FromStr` are guaranteed to round-trip.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpanId {
    pub(crate) tracing_id: tracing::span::Id,
    pub(crate) instance_id: u64,
}

impl SpanId {
    /// Metadata field name associated with `SpanId` values.
    pub fn meta_field_name() -> &'static str {
        "span-id"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseSpanIdError {
    ParseIntError(std::num::ParseIntError),
    FormatError,
}

impl FromStr for SpanId {
    type Err = ParseSpanIdError;

    /// Parses a Span Id from a `{SPAN}-{INSTANCE}` u64 pair, such as `1234567890-1234567890`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split('-');
        let s1 = iter.next().ok_or(ParseSpanIdError::FormatError)?;
        let u1 = u64::from_str_radix(s1, 10).map_err(ParseSpanIdError::ParseIntError)?;
        let s2 = iter.next().ok_or(ParseSpanIdError::FormatError)?;
        let u2 = u64::from_str_radix(s2, 10).map_err(ParseSpanIdError::ParseIntError)?;

        Ok(SpanId {
            tracing_id: tracing::Id::from_u64(u1),
            instance_id: u2,
        })
    }
}

impl Display for SpanId {
    /// Formats a Span Id as a `{SPAN}-{INSTANCE}` u64 pair, such as `1234567890-1234567890`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.tracing_id.into_u64(), self.instance_id)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    use crate::SpanId;

    proptest! {
        #[test]
        // ua is [1..] and not [0..] because 0 is not a valid tracing::Id (tracing::from_u64 throws on 0)
        fn span_id_round_trip(ua in 1u64.., ub in 1u64..) {
            let span_id = SpanId {
                tracing_id: tracing::Id::from_u64(ua),
                instance_id: ub,
            };
            let s = span_id.to_string();
            let res = SpanId::from_str(&s);
            assert_eq!(Ok(span_id), res);
        }
    }
}
