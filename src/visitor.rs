use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};
use crate::trace;

// visitor that builds honeycomb-compatible values from tracing fields
#[derive(Default)]
pub struct HoneycombVisitor(pub(crate) HashMap<String, Value>);

// reserved field names (TODO: document)
static RESERVED_WORDS: [&str; 9] = [
    "trace.span_id",
    "trace.trace_id",
    "trace.parent_id",
    "service_name",
    "level",
    "Timestamp",
    "name",
    "target",
    "duration_ms",
];


impl Visit for HoneycombVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let s = format!("{:?}", value);
        self.0
            .insert(mk_field_name(field.name().to_string()), json!(s));
    }
}

fn mk_field_name(s: String) -> String {
    // TODO: do another pass, optimize for efficiency (lazy static set?)
    if RESERVED_WORDS.contains(&&s[..]) {
        format!("tracing.{}", s)
    } else {
        s
    }
}


pub(crate) fn event_to_values(event: trace::Event<HoneycombVisitor>) -> HashMap<String, libhoney::Value> {
    let mut values = event.values.0;

    values.insert(
        // magic honeycomb string (trace.trace_id)
        "trace.trace_id".to_string(),
        // using explicit trace id passed in from ctx (req'd for lazy eval)
        json!(event.trace_id.0),
    );

    values.insert(
        // magic honeycomb string (trace.parent_id)
        "trace.parent_id".to_string(),
        event.parent_id
            .map(|pid| json!(format!("span-{}", pid.to_string())))
            .unwrap_or(json!(null)),
    );

    // magic honeycomb string (service_name)
    values.insert("service_name".to_string(), json!(event.service_name));

    values.insert("level".to_string(), json!(format!("{}", event.level)));

    values.insert(
        "Timestamp".to_string(),
        json!(event.initialized_at.to_rfc3339()),
    );

    // not honeycomb-special but tracing-provided
    values.insert("name".to_string(), json!(event.name));
    values.insert("target".to_string(), json!(event.target));

    values
}

pub(crate) fn span_to_values(span: trace::Span<HoneycombVisitor>) -> HashMap<String, libhoney::Value> {
    let mut values = span.values.0;

    values.insert(
        // magic honeycomb string (trace.span_id)
        "trace.span_id".to_string(),
        json!(format!("span-{}", span.id.to_string())),
    );

    values.insert(
        // magic honeycomb string (trace.trace_id)
        "trace.trace_id".to_string(),
        // using explicit trace id passed in from ctx (req'd for lazy eval)
        json!(span.trace_id.0),
    );

    values.insert(
        // magic honeycomb string (trace.parent_id)
        "trace.parent_id".to_string(),
        span.parent_id
            .map(|pid| json!(format!("span-{}", pid.to_string())))
            .unwrap_or(json!(null)),
    );

    // magic honeycomb string (service_name)
    values.insert("service_name".to_string(), json!(span.service_name));

    values.insert("level".to_string(), json!(format!("{}", span.level)));

    values.insert(
        "Timestamp".to_string(),
        json!(span.initialized_at.to_rfc3339()),
    );

    // not honeycomb-special but tracing-provided
    values.insert("name".to_string(), json!(span.name));
    values.insert("target".to_string(), json!(span.target));

    // honeycomb-special (I think, todo: get full list of known values)
    values.insert("duration_ms".to_string(), json!(span.elapsed_ms));

    values
}


