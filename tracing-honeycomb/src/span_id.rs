use std::convert::TryFrom;
use std::fmt::{self, Display};
use std::num::{NonZeroU64, ParseIntError, TryFromIntError};
use std::str::FromStr;
/// Unique Span identifier.
///
/// Wraps a `tracing::span::Id` with a suitable parser.
///
/// `Display` and `FromStr` are guaranteed to round-trip.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpanId {
    pub(crate) tracing_id: tracing::span::Id,
}

impl SpanId {
    /// Metadata field name associated with `SpanId` values.
    pub fn meta_field_name() -> &'static str {
        "span-id"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseSpanIdError {
    ParseIntError(ParseIntError),
    TryFromIntError(TryFromIntError),
}

impl Display for ParseSpanIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseIntError(e) => write!(f, "{}", e),
            Self::TryFromIntError(e) => write!(f, "{}", e),
        }
    }
}

impl From<ParseIntError> for ParseSpanIdError {
    fn from(err: ParseIntError) -> Self {
        Self::ParseIntError(err)
    }
}

impl From<TryFromIntError> for ParseSpanIdError {
    fn from(err: TryFromIntError) -> Self {
        Self::TryFromIntError(err)
    }
}

impl FromStr for SpanId {
    type Err = ParseSpanIdError;

    /// Parses a Span Id from a hex value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw_id = u64::from_str_radix(s, 16)?;
        let id = NonZeroU64::try_from(raw_id)?;

        Ok(SpanId {
            tracing_id: tracing::Id::from_non_zero_u64(id),
        })
    }
}

impl Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.tracing_id.into_u64())
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
        fn span_id_round_trip(ua in 1u64..) {
            let span_id = SpanId {
                tracing_id: tracing::Id::from_u64(ua),
            };
            let s = span_id.to_string();
            let res = SpanId::from_str(&s);
            assert_eq!(Ok(span_id), res);
        }
    }
}
