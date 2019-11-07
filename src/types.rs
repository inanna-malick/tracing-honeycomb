use crate::telemetry_subscriber::TelemetrySubscriber;
use ::libhoney::{json, Value};
use chrono::{DateTime, Utc};
use rand::Rng;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use tracing::field::Visit;
use tracing::span::{Attributes, Id};

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct SpanId(Id, u64);

impl SpanId {
    // TODO: idk, trait?
    fn into_string(self) -> String {
        format!("{}-{}", self.0.into_u64(), self.1)
    }

    fn from_id(id: Id, instance_id: u64) -> Self {
        SpanId(id, instance_id)
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceId(String);

impl TraceId {
    pub fn record_on_current_span(self) {
        let mut id = Some(self);
        tracing::dispatcher::get_default(|d| {
            if let Some(s) = d.downcast_ref::<TelemetrySubscriber>() {
                // required b/c get_default takes FnMut, however we know it will only be invoked once
                let id = id.take().expect("fn should not be invoked twice");
                s.record_trace_id(id)
            } else {
                // TODO: does this merit a panic? probably yes.
                panic!("unable to record TraceId, this thread does not have TelemetrySubscriber registered as default")
            }
        })
    }

    pub fn new(u: String) -> Self {
        TraceId(u)
    }
    pub(crate) fn generate() -> Self {
        let u: u128 = rand::thread_rng().gen();
        TraceId(format!("trace-{}", u))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Span<'a> {
    pub(crate) id: SpanId,
    pub(crate) trace_id: TraceId,
    pub(crate) parent_id: Option<SpanId>,
    pub(crate) initialized_at: DateTime<Utc>,
    pub(crate) elapsed_ms: i64,
    pub(crate) level: tracing::Level,
    pub(crate) name: &'a str,
    pub(crate) target: &'a str,
    pub(crate) service_name: &'a str,
    pub(crate) values: HashMap<String, Value>, // bag of misc values
}

impl<'a> Span<'a> {
    #[cfg(test)]
    pub(crate) fn into_static(self) -> Span<'static> {
        let e: Span<'static> = Span {
            name: lift_to_static(self.name),
            target: lift_to_static(self.target),
            service_name: lift_to_static(self.service_name),
            id: self.id,
            trace_id: self.trace_id,
            parent_id: self.parent_id,
            initialized_at: self.initialized_at,
            elapsed_ms: self.elapsed_ms,
            level: self.level,
            values: self.values,
        };
        e
    }

    pub(crate) fn into_values(self) -> HashMap<String, Value> {
        let mut values = self.values;

        values.insert(
            // magic honeycomb string (trace.span_id)
            "trace.span_id".to_string(),
            json!(format!("span-{}", self.id.into_string())),
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
                .map(|pid| json!(format!("span-{}", pid.into_string())))
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

// copy strings into lazy static ref in tests

#[derive(Clone, Debug)]
pub(crate) struct Event<'a> {
    pub(crate) trace_id: TraceId,
    pub(crate) parent_id: Option<SpanId>,
    pub(crate) initialized_at: DateTime<Utc>,
    pub(crate) level: tracing::Level,
    pub(crate) name: &'a str,
    pub(crate) target: &'a str,
    pub(crate) service_name: &'a str,
    pub(crate) values: HashMap<String, Value>, // bag of misc values
}

impl<'a> Event<'a> {
    #[cfg(test)]
    pub(crate) fn into_static(self) -> Event<'static> {
        let e: Event<'static> = Event {
            name: lift_to_static(self.name),
            target: lift_to_static(self.target),
            service_name: lift_to_static(self.service_name),
            trace_id: self.trace_id,
            parent_id: self.parent_id,
            initialized_at: self.initialized_at,
            level: self.level,
            values: self.values,
        };
        e
    }

    pub(crate) fn into_values(self) -> HashMap<String, Value> {
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
                .map(|pid| json!(format!("span-{}", pid.into_string())))
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
pub(crate) struct SpanData {
    pub(crate) lazy_trace_id: Option<TraceId>, // option used to impl cached lazy eval
    pub(crate) parent_id: Option<Id>,
    pub(crate) initialized_at: DateTime<Utc>,
    pub(crate) metadata: &'static tracing::Metadata<'static>,
    pub(crate) values: HashMap<String, Value>,
}

impl SpanData {
    pub(crate) fn into_span<'a>(
        self,
        elapsed_ms: i64,
        service_name: &'a str,
        instance_id: u64,
        trace_id: TraceId,
        id: Id,
    ) -> Span<'a> {
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
            name: metadata.name(),
            target: metadata.target(),
            level: metadata.level().clone(), // copy on inner type
            parent_id: parent_id.map(|i| SpanId::from_id(i, instance_id)),
            id: SpanId::from_id(id, instance_id),
            trace_id,
            initialized_at,
            elapsed_ms,
            values,
            service_name,
        }
    }

    pub(crate) fn into_event<'a>(
        self,
        service_name: &'a str,
        instance_id: u64,
        trace_id: TraceId,
    ) -> Event<'a> {
        let SpanData {
            parent_id,
            initialized_at,
            values,
            metadata,
            ..
        } = self;
        Event {
            // TODO: pull any other useful values out of metadata
            name: metadata.name(),
            target: metadata.target(),
            level: metadata.level().clone(), // copy on inner type
            parent_id: parent_id.map(|i| SpanId::from_id(i, instance_id)),
            trace_id,
            initialized_at,
            values,
            service_name,
        }
    }
}

/// ref-counted wrapper around some inner value 'T' used to manually
/// count references and trigger behavior when `ref_ct` reaches 0
pub(crate) struct RefCt<T> {
    pub(crate) ref_ct: usize,
    pub(crate) inner: T,
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
pub(crate) trait TelemetryObject {
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

#[cfg(test)]
fn lift_to_static(s: &'_ str) -> &'static str {
    // couldn't use mutex,
    // ref into vec was scoped on mutex _lock_
    // (b/c removal possible)
    use aovec::Aovec;
    lazy_static! {
        static ref STATIC_STRING_STORAGE: Aovec<String> = Aovec::new(256);
    }

    let idx = STATIC_STRING_STORAGE.push(s.to_string());
    STATIC_STRING_STORAGE.get(idx).unwrap()
}
