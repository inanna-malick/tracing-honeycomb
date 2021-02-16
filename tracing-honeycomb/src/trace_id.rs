use std::borrow::Cow;
use std::convert::{Infallible, TryInto};
use std::fmt::{self, Display};
use std::str::FromStr;

use uuid::Uuid;

/// A Honeycomb Trace ID.
///
/// Uniquely identifies a single distributed trace.
///
/// Does no parsing on string input values. Can be generated new from a UUID V4.
///
/// `Display` and `FromStr` are guaranteed to round-trip.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TraceId(pub(crate) String);

impl TraceId {
    /// Metadata field name associated with this `TraceId` values.
    pub fn meta_field_name() -> &'static str {
        "trace-id"
    }

    /// Generate a new `TraceId` from a UUID V4.
    pub fn new() -> Self {
        Uuid::new_v4().into()
    }

    #[deprecated(since = "0.2.0", note = "Use `TraceId::new()` instead.")]
    /// Generate a new `TraceId` from a UUID V4.
    ///
    /// Prefer `TraceId::new()`.
    pub fn generate() -> Self {
        TraceId::new()
    }
}

impl Default for TraceId {
    fn default() -> Self {
        TraceId::new()
    }
}

impl AsRef<[u8]> for TraceId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Into<String> for TraceId {
    fn into(self) -> String {
        format!("{}", self)
    }
}

impl TryInto<u128> for TraceId {
    type Error = uuid::Error;

    fn try_into(self) -> Result<u128, Self::Error> {
        Ok(Uuid::parse_str(&self.0)?.as_u128())
    }
}

impl TryInto<Uuid> for TraceId {
    type Error = uuid::Error;

    fn try_into(self) -> Result<Uuid, Self::Error> {
        Ok(Uuid::parse_str(&self.0)?)
    }
}

impl From<Cow<'_, &str>> for TraceId {
    fn from(s: Cow<'_, &str>) -> Self {
        Self(s.to_string())
    }
}

impl From<&str> for TraceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for TraceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<Uuid> for TraceId {
    fn from(uuid: Uuid) -> Self {
        let buf = &mut [0; 36];
        let id = uuid.to_simple().encode_lower(buf);
        Self(id.to_owned())
    }
}

impl From<u128> for TraceId {
    fn from(u: u128) -> Self {
        Uuid::from_u128(u).into()
    }
}

impl FromStr for TraceId {
    type Err = Infallible;

    /// Is actually infalliable.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

impl Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        #[test]
        fn trace_id_round_trip_u128(u in 1u128..) {
            let trace_id: TraceId = u.into();
            let s = trace_id.to_string();
            let res = TraceId::from_str(&s);
            assert_eq!(Ok(trace_id), res);
        }
    }

    #[test]
    fn trace_id_round_trip_str() {
        let trace_id: TraceId = "a string".into();
        let s = trace_id.to_string();
        let res = TraceId::from_str(&s);
        assert_eq!(Ok(trace_id), res);
    }

    #[test]
    fn trace_id_round_trip_empty_str() {
        let trace_id: TraceId = "".into();
        let s = trace_id.to_string();
        let res = TraceId::from_str(&s);
        assert_eq!(Ok(trace_id), res);
    }
}
