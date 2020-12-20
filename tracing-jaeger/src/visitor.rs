use opentelemetry::api::core::KeyValue;
use opentelemetry::api::core::Value;
use opentelemetry::api::trace::{
    self,
    span_context::{SpanContext, SpanId, TraceId},
};
use opentelemetry::api::SpanKind;
use opentelemetry::exporter::trace::SpanData;
use opentelemetry::sdk::trace::evicted_hash_map::EvictedHashMap;
use opentelemetry::sdk::trace::evicted_queue::EvictedQueue;
use opentelemetry::sdk::Resource;
use std::fmt;
use std::sync::Arc;
use tracing::field::{Field, Visit};
use tracing_distributed::{Event, Span};

/// PROBLEM: need 'opentelemetry::sdk::trace::config::Config' for 'max_events_per_span' value

/// Visitor that builds honeycomb-compatible values from tracing fields.
#[derive(Debug)]
#[doc(hidden)]
pub struct OpenTelemetryVisitor(pub(crate) EvictedHashMap);

impl Visit for OpenTelemetryVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0
            .insert(KeyValue::new(field.name(), Value::I64(value)))
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0
            .insert(KeyValue::new(field.name(), Value::U64(value)))
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0
            .insert(KeyValue::new(field.name(), Value::Bool(value)))
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(KeyValue::new(field.name(), value))
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let s = format!("{:?}", value);
        self.0.insert(KeyValue::new(field.name(), Value::String(s)))
    }
}

pub(crate) fn event_to_values(
    event: Event<OpenTelemetryVisitor, SpanId, TraceId>,
) -> trace::event::Event {
    let mut attributes = event.values.0;

    // NOTE: present on parent span_data, no need to include here
    // magic honeycomb string (service_name)
    // values.insert("service_name".to_string(), json!(event.service_name));

    attributes.insert(KeyValue::new("event.level", event.meta.level().to_string()));
    attributes.insert(KeyValue::new("event.target", event.meta.target()));

    // FIXME/TODO: would be cool to store name as static(?) (I think) string until passed over to exporter
    // FIXME/TODO: instead of to_string
    trace::event::Event {
        name: event.meta.name().to_string(),
        timestamp: event.initialized_at,
        attributes: attributes
            .into_iter()
            .map(|(k, v)| KeyValue::new(k, v))
            .collect(),
    }
}

pub(crate) fn span_to_values(
    span: Span<OpenTelemetryVisitor, SpanId, TraceId>,
    events: EvictedQueue<trace::event::Event>,
) -> SpanData {
    let mut attributes = span.values.0;

    attributes.insert(KeyValue::new("span.level", span.meta.level().to_string()));

    attributes.insert(KeyValue::new("span.service_name", span.service_name));

    attributes.insert(KeyValue::new("span.target", span.meta.target()));

    // TODO: traceflags 0? Is that no flags? hope so
    // TODO: examine use of is_remote
    SpanData {
        span_context: SpanContext::new(span.trace_id, span.id, 0, false),
        parent_span_id: span.parent_id.unwrap_or_else(SpanId::invalid), // idea: invalid == no parent, for root span
        span_kind: SpanKind::Internal,
        name: span.meta.name().to_string(),
        start_time: span.initialized_at,
        end_time: span.completed_at,
        attributes,
        message_events: events,
        links: EvictedQueue::new(0),
        status_code: opentelemetry::api::trace::span::StatusCode::OK, // TODO: not sure how to get this from tracing
        status_message: "".to_string(), // FIXME/TODO: put something useful here
        // TODO/FIXME: figure out a way to get global info (eg service name) in shared resource
        resource: Arc::new(Resource::new(std::iter::empty())),
    }
}
