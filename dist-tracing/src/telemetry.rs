use crate::trace::{Event, Span};

pub trait Telemetry {
    type Visitor: Default + tracing::field::Visit;

    fn report_span<'a>(&self, span: Span<'a, Self::Visitor>);
    fn report_event<'a>(&self, event: Event<'a, Self::Visitor>);
}

#[derive(Default)]
pub struct BlackholeVisitor;

impl tracing::field::Visit for BlackholeVisitor {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

pub struct BlackholeTelemetry;

impl Telemetry for BlackholeTelemetry {
    type Visitor = BlackholeVisitor;
    fn report_span(&self, _: Span<Self::Visitor>) {}

    fn report_event(&self, _: Event<Self::Visitor>) {}
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    /// Mock telemetry capability
    pub struct TestTelemetry<V> {
        spans: Arc<Mutex<Vec<Span<'static, V>>>>,
        events: Arc<Mutex<Vec<Event<'static, V>>>>,
    }

    impl<V> TestTelemetry<V> {
        pub fn new(
            spans: Arc<Mutex<Vec<Span<'static, V>>>>,
            events: Arc<Mutex<Vec<Event<'static, V>>>>,
        ) -> Self {
            TestTelemetry { spans, events }
        }
    }

    impl<V: tracing::field::Visit + Default> Telemetry for TestTelemetry<V> {
        type Visitor = V;

        fn report_span(&self, span: Span<V>) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut spans = self.spans.lock().unwrap();
            spans.push(span.into_static());
        }

        fn report_event(&self, event: Event<V>) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut events = self.events.lock().unwrap();
            events.push(event.into_static());
        }
    }
}
