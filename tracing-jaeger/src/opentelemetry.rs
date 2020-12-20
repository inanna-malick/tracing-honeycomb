use crate::visitor::{event_to_values, span_to_values, OpenTelemetryVisitor};
use opentelemetry::api::trace::{self, span_context::SpanId, span_context::TraceId};
use opentelemetry::sdk::trace::config::Config;
use opentelemetry::sdk::trace::evicted_hash_map::EvictedHashMap;
use opentelemetry::sdk::EvictedQueue;
use std::collections::HashMap;
use std::sync::Arc;
use tracing_distributed::{Event, Span, Telemetry};

#[cfg(not(feature = "use_parking_lot"))]
use std::sync::Mutex;
#[cfg(feature = "use_parking_lot")]
use parking_lot::Mutex;

/// Telemetry capability that publishes events and spans to some OpenTelemetry backend
#[derive(Debug)]
pub struct OpenTelemetry {
    pub(crate) exporter: Box<dyn opentelemetry::exporter::trace::SpanExporter>,
    // TODO: should have some eviction strategy so this doesn't grow forever
    pub(crate) events: Mutex<HashMap<SpanId, EvictedQueue<trace::event::Event>>>,
    pub(crate) config: Config,
}

impl Telemetry for OpenTelemetry {
    type Visitor = OpenTelemetryVisitor;
    type TraceId = TraceId;
    type SpanId = SpanId;

    // FIXME/NOTE: each event on a span is also allowed to have max_attributes_per_span attrs
    fn mk_visitor(&self) -> Self::Visitor {
        OpenTelemetryVisitor(EvictedHashMap::new(self.config.max_attributes_per_span))
    }

    fn report_span(&self, span: Span<Self::Visitor, Self::SpanId, Self::TraceId>) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        #[cfg(not(feature = "use_parking_lot"))]
        let mut events = self.events.lock().unwrap();
        #[cfg(feature = "use_parking_lot")]
        let mut events = self.events.lock();

        let events = events
            .remove(&span.id)
            .unwrap_or_else(|| EvictedQueue::new(0));
        let data = span_to_values(span, events);
        self.exporter.export(vec![Arc::new(data)]); // TODO: batch
    }

    fn report_event(&self, event: Event<Self::Visitor, Self::SpanId, Self::TraceId>) {
        // events are reported as part of spandata, event must have a parent to be recorded
        if let Some(id) = event.parent_id {
            #[cfg(not(feature = "use_parking_lot"))]
            let mut events = self.events.lock().unwrap();
            #[cfg(feature = "use_parking_lot")]
            let mut events = self.events.lock();

            if let Some(q) = events.get_mut(&id) {
                q.append_vec(&mut vec![event_to_values(event)]);
            } else {
                let mut q = EvictedQueue::new(self.config.max_events_per_span);
                q.append_vec(&mut vec![event_to_values(event)]);
                events.insert(id, q);
            }
        }
    }
}
