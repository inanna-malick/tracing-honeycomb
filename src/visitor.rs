use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};

// visitor that builds honeycomb-compatible values from tracing fields
pub struct HoneycombVisitor<'a> {
    pub accumulator: &'a mut HashMap<String, Value>,
}

// TODO: use field name ref instead of full string as map key
impl<'a> Visit for HoneycombVisitor<'a> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        // using 'telemetry.' namespace to disambiguate from system-level names
        let field_name = format!("telemetry.{}", field.name());
        self.accumulator.insert(field_name, json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        // using 'telemetry.' namespace to disambiguate from system-level names
        let field_name = format!("telemetry.{}", field.name());
        self.accumulator.insert(field_name, json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        // using 'telemetry.' namespace to disambiguate from system-level names
        let field_name = format!("telemetry.{}", field.name());
        self.accumulator.insert(field_name, json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        // using 'telemetry.' namespace to disambiguate from system-level names
        let field_name = format!("telemetry.{}", field.name());
        self.accumulator.insert(field_name, json!(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let s = format!("{:?}", value);
        // using 'telemetry.' namespace to disambiguate from system-level names
        let field_name = format!("telemetry.{}", field.name());
        self.accumulator.insert(field_name, json!(s));
    }
}
