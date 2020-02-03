use libhoney::FieldHolder;
use std::collections::HashMap;
use std::sync::Mutex;
use crate::visitor::{HoneycombVisitor, span_to_values, event_to_values};
use crate::trace::{Event, Span};

pub trait Telemetry {
    type Visitor: Default + tracing::field::Visit;

    fn report_span<'a>(&self, span: Span<'a, Self::Visitor>);
    fn report_event<'a>(&self, event: Event<'a, Self::Visitor>);
}

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
    fn report_span(&self, span: Span<Self::Visitor>) {
        let data = span_to_values(span);
        self.report_data(data);
    }

    fn report_event(&self, event: Event<Self::Visitor>) {
        let data = event_to_values(event);
        self.report_data(data);
    }
}


#[derive(Default)]
pub struct BlackholeVisitor;

impl tracing::field::Visit for BlackholeVisitor {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {
    }
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
