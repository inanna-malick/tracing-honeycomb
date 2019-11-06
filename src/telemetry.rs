use crate::types::{Event, Span};
use libhoney::FieldHolder;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait TelemetryCap {
    fn report_span<'a>(&self, span: Span<'a>);
    fn report_event<'a>(&self, event: Event<'a>);
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

impl TelemetryCap for HoneycombTelemetry {
    fn report_span(&self, span: Span) {
        let data = span.into_values();
        self.report_data(data);
    }

    fn report_event(&self, event: Event) {
        let data = event.into_values();
        self.report_data(data);
    }
}

#[cfg(test)]
pub mod test {
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

    impl TelemetryCap for TestTelemetry {
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
