use chrono::{DateTime, Utc};
use libhoney::{json, FieldHolder};
use std::collections::HashMap;
use std::sync::Mutex;
use crate::visitor::{HoneycombVisitor, span_to_values, event_to_values};
use tracing_subscriber::registry::LookupSpan;
use crate::trace::{Event, Span};

pub trait Telemetry {
    // the value of the associated type `V` (from the trait `telemetry::Telemetry`) must be specified
    // problem: associated type makes dyn awkward? how to handle..?

    // Q: can I make this opaque from the outside? I don't need to know what it is, so this _should_ be fine
    // specific need: carry around vtable for Telemetry
    // basically just an interrelated tuple of dyn
    type Visitor: Default + tracing::field::Visit;


    // ok, so how do I avoid having the type of V on TelemetryLayer? Needs to be mono b/c type coercion..

    // can't just pass in a function that takes some boxed visitor b/c it needs to _accumulate_ values
    // over multiple observe-on type whatsits for a single span, so it basically needs to be a K/V hashmap

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

pub struct BlackholeTelemetry;

impl Telemetry for BlackholeTelemetry {
    type Visitor = HoneycombVisitor;
    fn report_span(&self, _: Span<Self::Visitor>) {}

    fn report_event(&self, _: Event<Self::Visitor>) {}
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::sync::Arc;

    /// Mock telemetry capability
    pub struct TestTelemetry {
        spans: Arc<Mutex<Vec<Span<'static>>>>,
        events: Arc<Mutex<Vec<Event<'static>>>>,
    }

    impl TestTelemetry {
        pub fn new(
            spans: Arc<Mutex<Vec<Span<'static>>>>,
            events: Arc<Mutex<Vec<Event<'static>>>>,
        ) -> Self {
            TestTelemetry { spans, events }
        }
    }

    impl Telemetry for TestTelemetry {
        fn report_span(&self, span: Span) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut spans = self.spans.lock().unwrap();
            spans.push(span.into_static());
        }

        fn report_event(&self, event: Event) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut events = self.events.lock().unwrap();
            events.push(event.into_static());
        }
    }
}
