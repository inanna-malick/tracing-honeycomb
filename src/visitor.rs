use crate::types::TraceId;
use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};

// magic tracing id field name - used to report tracing id to span;
pub static MAGIC_TRACING_ID_FIELD_NAME: &'static str = "magic_tracing_id_field_name";

// just clone values into telemetry-appropriate hash map
pub struct HoneycombVisitor<'a> {
    pub accumulator: &'a mut HashMap<String, Value>,
    pub explicit_trace_id: Option<TraceId>,
}

impl<'a> Visit for HoneycombVisitor<'a> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MAGIC_TRACING_ID_FIELD_NAME {
            println!("found explicit trace id {}", &value);
            self.explicit_trace_id = Some(TraceId::new(value.to_string()));
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
