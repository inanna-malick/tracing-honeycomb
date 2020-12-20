use crate::visitor::{event_to_values, span_to_values, HoneycombVisitor};
use libhoney::FieldHolder;
use std::collections::HashMap;
use std::str::FromStr;
use tracing_distributed::{Event, Span, Telemetry};

#[cfg(feature = "use_parking_lot")]
use parking_lot::Mutex;
#[cfg(not(feature = "use_parking_lot"))]
use std::sync::Mutex;

/// Telemetry capability that publishes events and spans to Honeycomb.io.
#[derive(Debug)]
pub struct HoneycombTelemetry {
    honeycomb_client: Mutex<libhoney::Client<libhoney::transmission::Transmission>>,
    sample_rate: Option<u128>,
}

impl HoneycombTelemetry {
    pub(crate) fn new(cfg: libhoney::Config, sample_rate: Option<u128>) -> Self {
        let honeycomb_client = libhoney::init(cfg);

        // publishing requires &mut so just mutex-wrap it
        // FIXME: may not be performant, investigate options (eg mpsc)
        let honeycomb_client = Mutex::new(honeycomb_client);

        HoneycombTelemetry {
            honeycomb_client,
            sample_rate,
        }
    }

    fn report_data(&self, data: HashMap<String, ::libhoney::Value>) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        #[cfg(not(feature = "use_parking_lot"))]
        let mut client = self.honeycomb_client.lock().unwrap();
        #[cfg(feature = "use_parking_lot")]
        let mut client = self.honeycomb_client.lock();

        let mut ev = client.new_event();
        ev.add(data);
        let res = ev.send(&mut client);
        if let Err(err) = res {
            // unable to report telemetry (buffer full) so log msg to stderr
            // TODO: figure out strategy for handling this (eg report data loss event)
            eprintln!("error sending event to honeycomb, {:?}", err);
        }
    }

    fn should_report(&self, trace_id: TraceId) -> bool {
        match self.sample_rate {
            Some(sample_rate) => trace_id.0 % sample_rate == 0,
            None => true,
        }
    }
}

impl Telemetry for HoneycombTelemetry {
    type Visitor = HoneycombVisitor;
    type TraceId = TraceId;
    type SpanId = SpanId;

    fn mk_visitor(&self) -> Self::Visitor {
        Default::default()
    }

    fn report_span(&self, span: Span<Self::Visitor, Self::SpanId, Self::TraceId>) {
        if self.should_report(span.trace_id) {
            let data = span_to_values(span);
            self.report_data(data);
        }
    }

    fn report_event(&self, event: Event<Self::Visitor, Self::SpanId, Self::TraceId>) {
        if self.should_report(event.trace_id) {
            let data = event_to_values(event);
            self.report_data(data);
        }
    }
}

/// Unique Span identifier.
///
/// Combines a span's `tracing::Id` with an instance identifier to avoid id collisions in distributed scenarios.
///
/// `Display` and `FromStr` are guaranteed to round-trip.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct SpanId {
    pub(crate) tracing_id: tracing::Id,
    pub(crate) instance_id: u64,
}

impl SpanId {
    /// Metadata field name associated with `SpanId` values.
    pub fn meta_field_name() -> &'static str {
        "span-id"
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ParseSpanIdError {
    ParseIntError(std::num::ParseIntError),
    FormatError,
}

impl FromStr for SpanId {
    type Err = ParseSpanIdError;

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

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.tracing_id.into_u64(), self.instance_id)
    }
}

/// A Honeycomb Trace ID.
///
/// Uniquely identifies a single distributed trace.
///
/// `Display` and `FromStr` are guaranteed to round-trip.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub struct TraceId(pub(crate) u128);

impl TraceId {
    /// Metadata field name associated with this `TraceId` values.
    pub fn meta_field_name() -> &'static str {
        "trace-id"
    }

    /// Generate a random trace ID by using a thread-level RNG to generate a u128
    pub fn generate() -> Self {
        use rand::Rng;
        let u: u128 = rand::thread_rng().gen();

        TraceId(u)
    }
}

impl FromStr for TraceId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let u = u128::from_str_radix(s, 10)?;
        Ok(Self(u))
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;
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

        #[test]
        fn trace_id_round_trip(u in 1u128..) {
            let trace_id = TraceId(u);
            let s = trace_id.to_string();
            let res = TraceId::from_str(&s);
            assert_eq!(Ok(trace_id), res);
        }
    }
}
