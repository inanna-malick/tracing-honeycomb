use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};
use crate::types::TraceId;

// just clone values into telemetry-appropriate hash map
pub struct HoneycombVisitor<'a> {
    pub accumulator: &'a mut HashMap<String, Value>,
    pub explicit_trace_id: Option<TraceId>,
}

impl<'a> Visit for HoneycombVisitor<'a> {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "trace_id".to_string() {
            println!("found explicit trace id {}", &value);
            self.explicit_trace_id = Some(TraceId::new(value));
        } else {
            self.accumulator
                .insert(format!("telemetry.{}", field.name()), json!(value));
        }
    }

    // TODO: special visitors for various formats that honeycomb.io supports
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        // todo: more granular, per-type, etc
        // TODO: mb don't store 1x field name per span, instead use fmt-style trick w/ field id's by reading metadata..
        let s = format!("{:?}", value); // using 'telemetry.' namespace to disambiguate from system-level names
        self.accumulator
            .insert(format!("telemetry.{}", field.name()), json!(s));
    }
}
