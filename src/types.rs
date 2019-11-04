use crate::telemetry::HoneycombTelemetry;
use crate::telemetry_subscriber::TelemetrySubscriber;
use ::libhoney::{json, Value};
use chrono::{DateTime, Utc};
use rand::Rng;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use tracing::field::Visit;
use tracing::span::{Attributes, Id};

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceId(String);

impl TraceId {
    pub fn record_on_current_span(self) {
        // telemetry used in non-test scenarios
        self.record_on_current_span_::<HoneycombTelemetry>();
    }

    #[cfg(test)]
    pub fn record_on_current_span_test(self) {
        // telemetry used in non-test scenarios
        self.record_on_current_span_::<crate::telemetry::TestTelemetry>();
    }

    fn record_on_current_span_<T: 'static>(self) {
        tracing::dispatcher::get_default(|d| {
            // non-test cases are always honeycomb telemetry, export alias to avoid exposing complexity to users
            if let Some(s) = d.downcast_ref::<TelemetrySubscriber<T>>() {
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

#[derive(Debug)]
pub struct Span {
    pub id: Id,
    pub trace_id: TraceId,
    pub parent_id: Option<Id>,
    pub initialized_at: DateTime<Utc>,
    pub elapsed_ms: u32,
    pub level: tracing::Level,
    pub name: String,
    pub target: String,
    pub service_name: String,
    pub values: HashMap<String, Value>, // bag of misc values
}

impl Span {
    pub fn into_values(self) -> HashMap<String, Value> {
        let mut values = self.values;

        values.insert(
            // magic honeycomb string (trace.span_id)
            "trace.span_id".to_string(),
            json!(format!("span-{}", self.id.into_u64())),
        );

        values.insert(
            // magic honeycomb string (trace.trace_id)
            "trace.trace_id".to_string(),
            // using explicit trace id passed in from ctx (req'd for lazy eval)
            json!(self.trace_id.0),
        );

        values.insert(
            // magic honeycomb string (trace.parent_id)
            "trace.parent_id".to_string(),
            self.parent_id
                .map(|pid| json!(format!("span-{}", pid.into_u64())))
                .unwrap_or(json!(null)),
        );

        // magic honeycomb string (service_name)
        values.insert("service_name".to_string(), json!(self.service_name));

        values.insert("level".to_string(), json!(format!("{}", self.level)));

        values.insert(
            "timestamp".to_string(),
            json!(self.initialized_at.to_rfc3339()),
        );

        // not honeycomb-special but tracing-provided
        values.insert("name".to_string(), json!(self.name));
        values.insert("target".to_string(), json!(self.target));

        // honeycomb-special (I think, todo: get full list of known values)
        values.insert("duration_ms".to_string(), json!(self.elapsed_ms));

        values
    }
}

#[derive(Debug)]
pub struct Event {
    pub trace_id: TraceId,
    pub parent_id: Option<Id>,
    pub initialized_at: DateTime<Utc>,
    pub level: tracing::Level,
    pub name: String,
    pub target: String,
    pub service_name: String,
    pub values: HashMap<String, Value>, // bag of misc values
}

impl Event {
    pub fn into_values(self) -> HashMap<String, Value> {
        let mut values = self.values;

        values.insert(
            // magic honeycomb string (trace.trace_id)
            "trace.trace_id".to_string(),
            // using explicit trace id passed in from ctx (req'd for lazy eval)
            json!(self.trace_id.0),
        );

        values.insert(
            // magic honeycomb string (trace.parent_id)
            "trace.parent_id".to_string(),
            self.parent_id
                .map(|pid| json!(format!("span-{}", pid.into_u64())))
                .unwrap_or(json!(null)),
        );

        // magic honeycomb string (service_name)
        values.insert("service_name".to_string(), json!(self.service_name));

        values.insert("level".to_string(), json!(format!("{}", self.level)));

        values.insert(
            "timestamp".to_string(),
            json!(self.initialized_at.to_rfc3339()),
        );

        // not honeycomb-special but tracing-provided
        values.insert("name".to_string(), json!(self.name));
        values.insert("target".to_string(), json!(self.target));

        values
    }
}

/// Used to track spans in memory
pub struct SpanData {
    pub lazy_trace_id: Option<TraceId>, // option used to impl cached lazy eval
    pub parent_id: Option<Id>,
    pub initialized_at: DateTime<Utc>,
    pub metadata: &'static tracing::Metadata<'static>,
    pub values: HashMap<String, Value>,
}

impl SpanData {
    pub fn into_span(
        self,
        elapsed_ms: u32,
        service_name: String,
        trace_id: TraceId,
        id: Id,
    ) -> Span {
        // let SpanData { .. } = self;
        let SpanData {
            parent_id,
            initialized_at,
            values,
            metadata,
            ..
        } = self;
        Span {
            // TODO: pull any other useful values out of metadata
            name: metadata.name().to_string(),
            target: metadata.target().to_string(),
            level: metadata.level().clone(),
            id,
            trace_id,
            parent_id,
            initialized_at,
            elapsed_ms,
            values,
            service_name,
        }
    }

    // TODO: try reporting event w/ parent trace id but no span id
    pub fn into_event(self, service_name: String, trace_id: TraceId) -> Event {
        let SpanData {
            parent_id,
            initialized_at,
            values,
            metadata,
            ..
        } = self;
        Event {
            // TODO: pull any other useful values out of metadata
            name: metadata.name().to_string(),
            target: metadata.target().to_string(),
            level: metadata.level().clone(),
            trace_id,
            parent_id,
            initialized_at,
            values,
            service_name,
        }
    }
}

/// ref-counted wrapper around some inner value 'T' used to manually
/// count references and trigger behavior when `ref_ct` reaches 0
pub struct RefCt<T> {
    pub ref_ct: usize,
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
    fn t_metadata(&self) -> &'static tracing::Metadata<'static>;
    fn t_is_root(&self) -> bool;
    fn t_parent(&self) -> Option<&Id>;
}

impl<'a> TelemetryObject for Attributes<'a> {
    fn t_record(&self, visitor: &mut dyn Visit) {
        self.record(visitor)
    }
    fn t_metadata(&self) -> &'static tracing::Metadata<'static> {
        self.metadata()
    }
    fn t_is_root(&self) -> bool {
        self.is_root()
    }
    fn t_parent(&self) -> Option<&Id> {
        self.parent()
    }
}

impl<'a> TelemetryObject for tracing::Event<'a> {
    fn t_record(&self, visitor: &mut dyn Visit) {
        self.record(visitor)
    }
    fn t_metadata(&self) -> &'static tracing::Metadata<'static> {
        self.metadata()
    }
    fn t_is_root(&self) -> bool {
        self.is_root()
    }
    fn t_parent(&self) -> Option<&Id> {
        self.parent()
    }
}
