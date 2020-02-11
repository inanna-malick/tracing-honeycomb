use crate::visitor::{event_to_values, span_to_values, HoneycombVisitor};
use dist_tracing::{Event, Span, Telemetry};
use libhoney::FieldHolder;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct HoneycombTelemetry {
    honeycomb_client: Mutex<libhoney::Client<libhoney::transmission::Transmission>>,
}

impl HoneycombTelemetry {
    pub fn new(cfg: libhoney::Config) -> Self {
        let honeycomb_client = libhoney::init(cfg);

        // publishing requires &mut so just mutex-wrap it
        // FIXME: may not be performant, investigate options (eg mpsc)
        let honeycomb_client = Mutex::new(honeycomb_client);

        HoneycombTelemetry { honeycomb_client }
    }

    fn report_data(&self, data: HashMap<String, ::libhoney::Value>) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        let mut client = self.honeycomb_client.lock().unwrap();
        let mut ev = client.new_event();
        ev.add(data);
        let res = ev.send(&mut client);
        if let Err(err) = res {
            // unable to report telemetry (buffer full) so log msg to stderr
            // TODO: figure out strategy for handling this (eg report data loss event)
            eprintln!("error sending event to honeycomb, {:?}", err);
        }
    }
}

impl Telemetry for HoneycombTelemetry {
    type Visitor = HoneycombVisitor;
    type TraceId = TraceId;
    type SpanId = SpanId;

    fn report_span(&self, span: Span<Self::Visitor, Self::SpanId, Self::TraceId>) {
        let data = span_to_values(span);
        self.report_data(data);
    }

    fn report_event(&self, event: Event<Self::Visitor, Self::SpanId, Self::TraceId>) {
        let data = event_to_values(event);
        self.report_data(data);
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct SpanId {
    pub tracing_id: tracing::Id,
    pub instance_id: u64,
}

// TODO: round trip property test for Display + FromString
impl SpanId {
    /// attempts to parse a SpanId from a string
    pub fn from_string(s: &str) -> Option<SpanId> {
        let mut iter = s.split('-');
        let s1 = iter.next()?;
        let u1 = u64::from_str_radix(s1, 10).ok()?;
        let s2 = iter.next()?;
        let u2 = u64::from_str_radix(s2, 10).ok()?;

        Some(SpanId {
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

/// NOTE: why not just have this be a u128? It'd be so much better...
/// A Honeycomb Trace ID. Uniquely identifies a single distributed (potentially multi-process) trace.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceId(pub(crate) u128);

impl TraceId {
    // TODO: instead, FromStr impl
    /// attempts to parse a TraceId from a string
    pub fn from_string(s: &str) -> Option<Self> {
        let u = u128::from_str_radix(s, 10).ok()?;
        Some(TraceId(u))
    }

    /// Generate a random trace ID by using a thread-level RNG to generate a u128
    pub fn generate() -> Self {
        use rand::Rng;
        let u: u128 = rand::thread_rng().gen();

        TraceId(u)
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
