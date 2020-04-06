use crate::trace::{Event, Span};
use std::marker::PhantomData;

/// Represents the ability to publish events and spans to some arbitrary backend.
pub trait Telemetry {
    /// Type used to record tracing fields.
    type Visitor: Default + tracing::field::Visit;
    /// Globally unique identifier, uniquely identifies a trace.
    type TraceId: Send + Sync + Clone;
    /// Identifies spans within a trace.
    type SpanId: Send + Sync + Clone;
    /// Report a `Span` to this Telemetry instance's backend.
    fn report_span(&self, span: Span<Self::Visitor, Self::SpanId, Self::TraceId>);
    /// Report an `Event` to this Telemetry instance's backend.
    fn report_event(&self, event: Event<Self::Visitor, Self::SpanId, Self::TraceId>);
}

/// Visitor that records no information when visiting tracing fields.
#[derive(Default, Debug)]
pub struct BlackholeVisitor;

impl tracing::field::Visit for BlackholeVisitor {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

/// Telemetry implementation that does not publish information to any backend.
/// For use in tests.
pub struct BlackholeTelemetry<S, T>(PhantomData<S>, PhantomData<T>);

impl<S, T> Default for BlackholeTelemetry<S, T> {
    fn default() -> Self {
        BlackholeTelemetry(PhantomData, PhantomData)
    }
}

impl<SpanId, TraceId> Telemetry for BlackholeTelemetry<SpanId, TraceId>
where
    SpanId: 'static + Clone + Send + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    type Visitor = BlackholeVisitor;
    type TraceId = TraceId;
    type SpanId = SpanId;

    fn report_span(&self, _: Span<Self::Visitor, Self::SpanId, Self::TraceId>) {}

    fn report_event(&self, _: Event<Self::Visitor, Self::SpanId, Self::TraceId>) {}
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    // simplified ID types
    pub(crate) type TraceId = u64;
    pub(crate) type SpanId = tracing::Id;

    /// Mock telemetry capability
    pub struct TestTelemetry {
        spans: Arc<Mutex<Vec<Span<BlackholeVisitor, SpanId, TraceId>>>>,
        events: Arc<Mutex<Vec<Event<BlackholeVisitor, SpanId, TraceId>>>>,
    }

    impl TestTelemetry {
        pub fn new(
            spans: Arc<Mutex<Vec<Span<BlackholeVisitor, SpanId, TraceId>>>>,
            events: Arc<Mutex<Vec<Event<BlackholeVisitor, SpanId, TraceId>>>>,
        ) -> Self {
            TestTelemetry { spans, events }
        }
    }

    impl Telemetry for TestTelemetry {
        type Visitor = BlackholeVisitor;
        type SpanId = SpanId;
        type TraceId = TraceId;

        fn report_span(&self, span: Span<BlackholeVisitor, SpanId, TraceId>) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut spans = self.spans.lock().unwrap();
            spans.push(span);
        }

        fn report_event(&self, event: Event<BlackholeVisitor, SpanId, TraceId>) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut events = self.events.lock().unwrap();
            events.push(event);
        }
    }
}
