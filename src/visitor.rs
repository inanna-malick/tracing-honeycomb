use ::libhoney::{json, Value};
use std::collections::HashMap;
use std::fmt;
use tracing::field::{Field, Visit};

// just clone values into telemetry-appropriate hash map
pub struct HoneycombVisitor<'a> {
    pub accumulator: &'a mut HashMap<String, Value>,
}

impl<'a> Visit for HoneycombVisitor<'a> {
    // TODO: special visitor fns for various formats that honeycomb.io supports
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        // TODO: mb don't store 1x field name per span, instead use fmt-style trick w/ field id's by reading metadata..
        let s = format!("{:?}", value);
        // using 'telemetry.' namespace to disambiguate from system-level names
        self.accumulator
            .insert(format!("telemetry.{}", field.name()), json!(s));
    }
}
