use crate::types::{Event, Span};
use libhoney::FieldHolder;
use std::collections::HashMap;
use std::sync::Mutex;
#[cfg(test)]
use std::sync::Arc;


pub trait TelemetryCap {
    fn report_span(&self, span: Span);
    fn report_event(&self, event: Event);
}

pub struct HoneycombTelemetry {
    honeycomb_client: Mutex<libhoney::Client<libhoney::transmission::Transmission>>,
}

impl HoneycombTelemetry {
    pub fn new(cfg: libhoney::Config) -> Self {
        let honeycomb_client = libhoney::init(cfg);

        // publishing requires &mut so just mutex-wrap it (FIXME; perf?)
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
            // unable to report telemetry so log msg to stderr
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

/// Mock telemetry capability
#[cfg(test)]
pub struct TestTelemetry {
    spans: Arc<Mutex<Vec<Span>>>,
    events: Arc<Mutex<Vec<Event>>>,
}

#[cfg(test)]
impl TestTelemetry {
    pub fn new(spans: Arc<Mutex<Vec<Span>>>, events: Arc<Mutex<Vec<Event>>>) -> Self {
        TestTelemetry { spans, events }
    }
}

#[cfg(test)]
impl TelemetryCap for TestTelemetry {
    fn report_span(&self, span: Span) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        let mut spans = self.spans.lock().unwrap();
        spans.push(span);
    }

    fn report_event(&self, event: Event) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }
}
