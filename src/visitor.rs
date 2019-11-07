use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};

// visitor that builds honeycomb-compatible values from tracing fields
pub(crate) struct HoneycombVisitor<'a> {
    pub(crate) accumulator: &'a mut HashMap<String, Value>,
}

// reserved field names (TODO: document)
static RESERVED_WORDS: [&'static str; 9] = [
    "trace.span_id",
    "trace.trace_id",
    "trace.parent_id",
    "service_name",
    "level",
    "timestamp",
    "name",
    "target",
    "duration_ms",
];

impl<'a> Visit for HoneycombVisitor<'a> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.accumulator
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.accumulator
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.accumulator
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.accumulator
            .insert(mk_field_name(field.name().to_string()), json!(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let s = format!("{:?}", value);
        self.accumulator
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
