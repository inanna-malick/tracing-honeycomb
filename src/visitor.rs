use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};

// visitor that builds honeycomb-compatible values from tracing fields
pub struct HoneycombVisitor<'a> {
    pub accumulator: &'a mut HashMap<String, Value>,
}

// TODO: figure out how to handle reserved words (honeycomb tracing.trace_id, etc type stuff)
impl<'a> Visit for HoneycombVisitor<'a> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.accumulator.insert(field.name().to_string(), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.accumulator.insert(field.name().to_string(), json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.accumulator.insert(field.name().to_string(), json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.accumulator.insert(field.name().to_string(), json!(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let s = format!("{:?}", value);
        self.accumulator.insert(field.name().to_string(), json!(s));
    }
}
