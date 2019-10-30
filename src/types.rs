use crate::telemetry_subscriber::TelemetrySubscriber;
use ::libhoney::{json, Value};
use chrono::{DateTime, Utc};
use rand::Rng;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use tracing::field::Visit;
use tracing::span::{Attributes, Id};
use tracing::{Event, Metadata};

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceId(String);

impl TraceId {
    pub fn record_on_current_span(self) {
        tracing::dispatcher::get_default(|d| {
            if let Some(s) = d.downcast_ref::<TelemetrySubscriber>() {
                s.record_trace_id(self.clone()) // FIXME: don't clone string here
            } else {
                println!("unable to record TraceId, this thread does not have TelemetrySubscriber registered as default")
            }
        })
    }

    pub fn new(u: String) -> Self {
        TraceId(u)
    }
    pub fn generate() -> Self {
        let u: u128 = rand::thread_rng().gen();
        TraceId(format!("trace-{}", u))
    }
}

/// Used to track spans in memory
pub struct SpanData {
    pub trace_id: Option<TraceId>, // option used to impl cached lazy eval
    pub parent_id: Option<Id>,
    pub initialized_at: DateTime<Utc>,
    pub metadata: &'static Metadata<'static>,
    pub values: HashMap<String, Value>,
}

impl SpanData {
    pub fn into_values(
        self,
        service_name: &str,
        trace_id: Option<TraceId>,
        id: &Id,
    ) -> HashMap<String, Value> {
        let mut values = self.values;
        values.insert(
            // magic honeycomb string (trace.span_id)
            "trace.span_id".to_string(),
            json!(format!("span-{}", id.into_u64())),
        );

        if let Some(trace_id) = trace_id {
            values.insert(
                // magic honeycomb string (trace.trace_id)
                "trace.trace_id".to_string(),
                // using explicit trace id passed in from ctx (req'd for lazy eval)
                json!(trace_id.0),
            );
        };

        values.insert(
            // magic honeycomb string (trace.parent_id)
            "trace.parent_id".to_string(),
            self.parent_id
                .map(|pid| json!(format!("span-{}", pid.into_u64())))
                .unwrap_or(json!(null)),
        );

        // magic honeycomb string (service_name)
        values.insert("service_name".to_string(), json!(service_name));

        values.insert(
            "level".to_string(),
            json!(format!("{}", self.metadata.level())),
        );

        values.insert(
            "timestamp".to_string(),
            json!(self.initialized_at.to_rfc3339()),
        );

        // not honeycomb-special but tracing-provided
        values.insert("name".to_string(), json!(self.metadata.name()));
        values.insert("target".to_string(), json!(self.metadata.target()));

        values
    }
}

/// ref-counted wrapper around some inner value 'T' used to manually
/// count references and trigger behavior when `ref_ct` reaches 0
pub struct RefCt<T> {
    pub ref_ct: u64,
    pub inner: T,
}

impl<T> Deref for RefCt<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for RefCt<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Shim so I can write code that abstracts over span/event
pub trait TelemetryObject {
    fn t_record(&self, visitor: &mut dyn Visit);
    fn t_metadata(&self) -> &'static Metadata<'static>;
    fn t_is_root(&self) -> bool;
    fn t_parent(&self) -> Option<&Id>;
}

impl<'a> TelemetryObject for Attributes<'a> {
    fn t_record(&self, visitor: &mut dyn Visit) {
        self.record(visitor)
    }
    fn t_metadata(&self) -> &'static Metadata<'static> {
        self.metadata()
    }
    fn t_is_root(&self) -> bool {
        self.is_root()
    }
    fn t_parent(&self) -> Option<&Id> {
        self.parent()
    }
}

impl<'a> TelemetryObject for Event<'a> {
    fn t_record(&self, visitor: &mut dyn Visit) {
        self.record(visitor)
    }
    fn t_metadata(&self) -> &'static Metadata<'static> {
        self.metadata()
    }
    fn t_is_root(&self) -> bool {
        self.is_root()
    }
    fn t_parent(&self) -> Option<&Id> {
        self.parent()
    }
}
