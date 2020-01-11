use chrono::{DateTime, Utc};
use libhoney::{json, FieldHolder};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing_subscriber::registry::LookupSpan;

pub(crate) trait Telemetry {
    fn report_span<'a>(&self, span: Span<'a>);
    fn report_event<'a>(&self, event: Event<'a>);
}

pub(crate) struct HoneycombTelemetry {
    honeycomb_client: Mutex<libhoney::Client<libhoney::transmission::Transmission>>,
}

impl HoneycombTelemetry {
    pub(crate) fn new(cfg: libhoney::Config) -> Self {
        let honeycomb_client = libhoney::init(cfg);

        // publishing requires &mut so just mutex-wrap it
        // FIXME: may not be performant, investigate options (eg mpsc)
        let honeycomb_client = Mutex::new(honeycomb_client);

        HoneycombTelemetry { honeycomb_client }
    }

    fn report_data(&self, data: HashMap<String, ::libhoney::Value>) {
        // succeed or die. failure is unrecoverable (mutex poisoned)
        let mut client = self.honeycomb_client.lock().unwrap();
        let mut ev = client.new_event();
        ev.add(data);
        let res = ev.send(&mut client);
        if let Err(err) = res {
            // unable to report telemetry (buffer full) so log msg to stderr
            // TODO: figure out strategy for handling this (eg report data loss event)
            eprintln!("error sending event to honeycomb, {:?}", err);
        }
    }
}

impl Telemetry for HoneycombTelemetry {
    fn report_span(&self, span: Span) {
        let data = span.into_values();
        self.report_data(data);
    }

    fn report_event(&self, event: Event) {
        let data = event.into_values();
        self.report_data(data);
    }
}

pub(crate) struct BlackholeTelemetry;

impl Telemetry for BlackholeTelemetry {
    fn report_span(&self, _: Span) {}

    fn report_event(&self, _: Event) {}
}

// TODO: review pub vs. pub(crate)
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceCtx {
    pub parent_span: Option<SpanId>,
    pub trace_id: TraceId,
}

impl TraceCtx {
    /// Record a trace ID on the current span. Requires that the currently registered dispatcher
    /// have a TelemetrySubscriber reachable via 'downcast_ref', otherwise will panic.
    // TODO: expose error to user here instead of panic (but show panic in example, fail fast still makes sense here)
    pub fn record_on_current_span(self) {
        let mut ctx = Some(self);
        tracing::dispatcher::get_default(|d| {
            // panic if currently registered subscriber is not of the expected type (traverses layers via downcast_ref)
            if let Some(s) = d.downcast_ref::<crate::telemetry_layer::TelemetryLayer>() {
                // required b/c get_default takes FnMut, however we know it will only be invoked once
                let ctx = ctx.take().expect("fn should not be invoked twice");
                let current_span_id = d
                    .current_span()
                    .id()
                    .expect("unable to record TraceCtx, no current span")
                    .clone();
                s.record_trace_ctx(ctx, current_span_id);
            } else {
                panic!("unable to record TraceCtx, TelemetryLayer not registered as tracing layer",)
            }
        });
    }

    pub fn eval_current_trace_ctx() -> Option<Self> {
        tracing::dispatcher::get_default(|d| {
            // panic if currently registered subscriber is not of the expected type (traverses layers via downcast_ref)
            let telemetry_layer = d
                .downcast_ref::<crate::telemetry_layer::TelemetryLayer>()
                .expect(
                "unable to eval current trace ctx, TelemetryLayer not registered as tracing layer",
            );

            let registry = d
                .downcast_ref::<tracing_subscriber::Registry>()
                .expect(
                    "unable to eval current trace ctx, Registry subscriber not registered as tracing subscriber",
                );

            let current_span_id = d
                .current_span()
                .id()
                .expect("unable to record TraceCtx, no current span")
                .clone();

            let iter = itertools::unfold(Some(current_span_id.clone()), |st| match st {
                Some(target_id) => {
                    let res = registry
                        .span(target_id)
                        .expect("span data not found during eval_ctx for eval_current_trace_ctx");
                    *st = res.parent().map(|x| x.id());
                    Some(res)
                }
                None => None,
            });

            telemetry_layer.eval_ctx(iter).map(|x| TraceCtx {
                parent_span: Some(SpanId {
                    tracing_id: current_span_id,
                    instance_id: telemetry_layer.instance_id,
                }),
                trace_id: x.trace_id,
            })
        })
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct SpanId {
    pub tracing_id: tracing::Id,
    pub instance_id: u64,
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.tracing_id.into_u64(), self.instance_id)
    }
}

/// A Honeycomb Trace ID. Uniquely identifies a single distributed (potentially multi-process) trace.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceId(pub String);

impl TraceId {
    /// Create a new trace ID wrapping some provided String
    pub fn new(u: String) -> Self {
        TraceId(u)
    }

    /// Generate a random trace ID by using a thread-level RNG to generate a u128
    pub fn generate() -> Self {
        use rand::Rng;

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
    pub(crate) values: HashMap<String, libhoney::Value>, // bag of misc values
}

impl<'a> Span<'a> {
    #[cfg(test)]
    pub(crate) fn into_static(self) -> Span<'static> {
        let e: Span<'static> = Span {
            name: test::lift_to_static(self.name),
            target: test::lift_to_static(self.target),
            service_name: test::lift_to_static(self.service_name),
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

    pub(crate) fn into_values(self) -> HashMap<String, libhoney::Value> {
        let mut values = self.values;

        values.insert(
            // magic honeycomb string (trace.span_id)
            "trace.span_id".to_string(),
            json!(format!("span-{}", self.id.to_string())),
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
                .map(|pid| json!(format!("span-{}", pid.to_string())))
                .unwrap_or(json!(null)),
        );

        // magic honeycomb string (service_name)
        values.insert("service_name".to_string(), json!(self.service_name));

        values.insert("level".to_string(), json!(format!("{}", self.level)));

        values.insert(
            "Timestamp".to_string(),
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

#[derive(Clone, Debug)]
pub(crate) struct Event<'a> {
    pub(crate) trace_id: TraceId,
    pub(crate) parent_id: Option<SpanId>,
    pub(crate) initialized_at: DateTime<Utc>,
    pub(crate) level: tracing::Level,
    pub(crate) name: &'a str,
    pub(crate) target: &'a str,
    pub(crate) service_name: &'a str,
    pub(crate) values: HashMap<String, libhoney::Value>, // bag of misc values
}

impl<'a> Event<'a> {
    #[cfg(test)]
    pub(crate) fn into_static(self) -> Event<'static> {
        let e: Event<'static> = Event {
            name: test::lift_to_static(self.name),
            target: test::lift_to_static(self.target),
            service_name: test::lift_to_static(self.service_name),
            trace_id: self.trace_id,
            parent_id: self.parent_id,
            initialized_at: self.initialized_at,
            level: self.level,
            values: self.values,
        };
        e
    }

    pub(crate) fn into_values(self) -> HashMap<String, libhoney::Value> {
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
                .map(|pid| json!(format!("span-{}", pid.to_string())))
                .unwrap_or(json!(null)),
        );

        // magic honeycomb string (service_name)
        values.insert("service_name".to_string(), json!(self.service_name));

        values.insert("level".to_string(), json!(format!("{}", self.level)));

        values.insert(
            "Timestamp".to_string(),
            json!(self.initialized_at.to_rfc3339()),
        );

        // not honeycomb-special but tracing-provided
        values.insert("name".to_string(), json!(self.name));
        values.insert("target".to_string(), json!(self.target));

        values
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::sync::Arc;

    pub(super) fn lift_to_static(s: &'_ str) -> &'static str {
        use aovec::Aovec;
        lazy_static! {
            static ref STATIC_STRING_STORAGE: Aovec<String> = Aovec::new(256);
        }

        let idx = STATIC_STRING_STORAGE.push(s.to_string());
        STATIC_STRING_STORAGE.get(idx).unwrap()
    }

    /// Mock telemetry capability
    pub(crate) struct TestTelemetry {
        spans: Arc<Mutex<Vec<Span<'static>>>>,
        events: Arc<Mutex<Vec<Event<'static>>>>,
    }

    impl TestTelemetry {
        pub(crate) fn new(
            spans: Arc<Mutex<Vec<Span<'static>>>>,
            events: Arc<Mutex<Vec<Event<'static>>>>,
        ) -> Self {
            TestTelemetry { spans, events }
        }
    }

    impl Telemetry for TestTelemetry {
        fn report_span(&self, span: Span) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut spans = self.spans.lock().unwrap();
            spans.push(span.into_static());
        }

        fn report_event(&self, event: Event) {
            // succeed or die. failure is unrecoverable (mutex poisoned)
            let mut events = self.events.lock().unwrap();
            events.push(event.into_static());
        }
    }
}
