use crate::trace::{Event, Span};
use std::marker::PhantomData;

pub trait Telemetry {
    type Visitor: Default + tracing::field::Visit;
    type TraceId: Send + Sync + Clone;
    type SpanId: Send + Sync + Clone;

    fn report_span(&self, span: Span<Self::Visitor, Self::SpanId, Self::TraceId>);
    fn report_event(&self, event: Event<Self::Visitor, Self::SpanId, Self::TraceId>);
}

#[derive(Default)]
pub struct BlackholeVisitor;

impl tracing::field::Visit for BlackholeVisitor {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

// NOTE: each crate needs to export one of these now, but whatevs

pub struct BlackholeTelemetry<S, T>(PhantomData<S>, PhantomData<T>);

impl<S, T> Default for BlackholeTelemetry<S, T> {
    fn default() -> Self {
        BlackholeTelemetry(PhantomData, PhantomData)
    }
}

impl<SpanId, TraceId> Telemetry for BlackholeTelemetry<SpanId, TraceId>
where
    SpanId: 'static + Send + Clone + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    type Visitor = BlackholeVisitor;
    type TraceId = TraceId;
    type SpanId = SpanId;

    // fn promote_span_id(id: tracing::span::Id) -> Self::SpanId;

    fn report_span(&self, _: Span<Self::Visitor, Self::SpanId, Self::TraceId>) {}

    fn report_event(&self, _: Event<Self::Visitor, Self::SpanId, Self::TraceId>) {}
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    // simplified ID types
    pub type TraceId = u64;
    pub type SpanId = tracing::Id;

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
