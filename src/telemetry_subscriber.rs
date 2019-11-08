use crate::telemetry::{HoneycombTelemetry, Telemetry};
use crate::types::{RefCt, SpanData, TelemetryObject, TraceId};
use crate::visitor::HoneycombVisitor;
use chrono::Utc;
use rand::Rng;
use sharded_slab::{Guard, Slab};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{RwLock, RwLockWriteGuard};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};
use tracing_core::span::Current;

// used within this subscriber to track the current span
thread_local! {
    static CURRENT_SPAN: RefCell<Vec<Id>> = RefCell::new(vec!());
}

// TODO: debug impl?
pub struct TelemetrySubscriber {
    telemetry: Box<dyn Telemetry + Send + Sync + 'static>,
    service_name: String,
    spans: Arc<Slab<RwLock<RefCt<SpanData>>>>,
    // used to construct span ids to avoid collisions
    instance_id: u64,
}

impl TelemetrySubscriber {
    pub fn new(service_name: String, config: libhoney::Config) -> Self {
        let telemetry = Box::new(HoneycombTelemetry::new(config));
        Self::new_(service_name, telemetry)
    }

    pub(crate) fn new_(
        service_name: String,
        telemetry: Box<dyn Telemetry + Send + Sync + 'static>,
    ) -> Self {
        let instance_id = rand::thread_rng().gen();

        TelemetrySubscriber {
            spans: Arc::new(Slab::new()), // uses default config
            instance_id,
            service_name,
            telemetry,
        }
    }

    pub(crate) fn record_trace_id(&self, trace_id: TraceId) {
        if let Some(id) = self.peek_current_span() {
            if let Some(rw_lock) = self.spans.get(id_to_idx(&id)) {
                let mut span_data = rw_lock.write().unwrap();
                // open questions:
                // - what if this node already has a trace id (currently overwrites, mb panic?)
                span_data.lazy_trace_id = Some(trace_id);
            }
        }
    }

    /// this function provides lazy initialization of trace ids (only generated when req'd to observe honeycomb event/span)
    /// when a span's trace id is requested, that span and any parent spans can have their trace id evaluated and saved
    fn get_or_gen_trace_id(&self, target_id: &Id) -> TraceId {
        let mut path: Vec<Guard<RwLock<RefCt<SpanData>>>> = vec![];

        let mut id = target_id.clone();

        let trace_id: TraceId = loop {
            if let Some(guard) = self.spans.get(id_to_idx(&id)) {
                let span = guard.read().unwrap();
                if let Some(tid) = &span.lazy_trace_id {
                    // found already-eval'd trace id
                    break tid.clone();
                } else {
                    // span has no trace, must be updated as part of this call
                    if let Some(parent_id) = &span.parent_id {
                        id = parent_id.clone();
                        drop(span);
                        path.push(guard);
                    } else {
                        // found root span with no trace id, generate trace_id
                        let trace_id = TraceId::generate();
                        drop(span);
                        path.push(guard);
                        break trace_id;
                    };
                };
            } else {
                // invalid state
                panic!("BUG[honeycomb-telemetry] unable to traverse link to parent span, span data not found");
            }
        };

        // get write guards for path
        let path: Vec<RwLockWriteGuard<RefCt<SpanData>>> =
            path.iter().map(|g| g.write().unwrap()).collect();

        // check to see if any write guard'd span has had its lazy_trace_id set since we read it
        if let Some(_) = path.iter().find(|span| span.lazy_trace_id.is_some()) {
            // if so, abort and retry. path now contains trace id
            self.get_or_gen_trace_id(target_id)
        } else {
            // otherwise, update each write guard'd span, setting their trace id
            for mut span in path {
                span.lazy_trace_id = Some(trace_id.clone());
            }

            trace_id
        }
    }

    fn peek_current_span(&self) -> Option<Id> {
        CURRENT_SPAN.with(|c| c.borrow().last().cloned())
    }
    fn pop_current_span(&self) -> Option<Id> {
        CURRENT_SPAN.with(|c| c.borrow_mut().pop())
    }
    fn push_current_span(&self, id: Id) {
        CURRENT_SPAN.with(|c| c.borrow_mut().push(id))
    }

    // get (trace_id, parent_id). will generate a new trace id if none are available
    fn build_span<X: TelemetryObject>(&self, t: &X) -> SpanData {
        let now = Utc::now();

        let mut values = HashMap::new();
        let mut visitor = HoneycombVisitor {
            accumulator: &mut values,
        };
        t.t_record(&mut visitor);

        let parent_id = if let Some(parent_id) = t.t_parent() {
            // explicit parent
            Some(parent_id.clone())
        } else if t.t_is_root() {
            // don't bother checking thread local if span is explicitly root according to this fn
            None
        } else if let Some(parent_id) = self.peek_current_span() {
            // implicit parent from threadlocal ctx
            Some(parent_id)
        } else {
            // no parent span, thus this is a root span
            None
        };

        SpanData {
            initialized_at: now,
            metadata: t.t_metadata(),
            lazy_trace_id: None, // not yet evaluated
            parent_id,
            values,
        }
    }
}

impl Subscriber for TelemetrySubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        let new_span = self.build_span(span);

        let idx: usize = self
            .spans
            .insert(RwLock::new(RefCt {
                ref_ct: 1,
                inner: new_span,
            }))
            .expect("unable to add span to slab (OOM?)");

        idx_to_id(idx)
    }

    // record additional values on span map
    fn record(&self, id: &Id, values: &Record<'_>) {
        if let Some(rw_lock) = self.spans.get(id_to_idx(id)) {
            let mut span_data = rw_lock.write().unwrap();
            let mut visitor = HoneycombVisitor {
                accumulator: &mut span_data.values,
            };
            values.record(&mut visitor);
        } else {
            println!(
                "no span in map when recording to span with id {:?}, possible bug",
                id
            )
        }
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    // record event (pub(crate)lish directly to telemetry, not a span)
    fn event(&self, event: &Event<'_>) {
        // report as span with zero-length interval
        let new_span = self.build_span(event);

        // use parent trace id, if it exists
        let trace_id = new_span
            .parent_id
            .as_ref()
            .map(|pid| self.get_or_gen_trace_id(pid))
            // if this event doesn't belong to a trace,
            // just generate a top-level trace id for it
            // TODO: consider allowing events w/ no trace id
            .unwrap_or_else(TraceId::generate);

        let event = new_span.into_event(&self.service_name, self.instance_id, trace_id);

        self.telemetry.report_event(event);
    }

    fn enter(&self, span: &Id) {
        self.push_current_span(span.clone());
    }
    fn exit(&self, _span: &Id) {
        self.pop_current_span();
    }

    fn clone_span(&self, id: &Id) -> Id {
        if let Some(rw_lock) = self.spans.get(id_to_idx(id)) {
            let mut span_data = rw_lock.write().unwrap();
            span_data.ref_ct += 1;
        }
        id.clone() // type sig of this function seems to compel cloning of id (&X -> X)
    }

    fn try_close(&self, id: Id) -> bool {
        let dropped_span: Option<(SpanData, TraceId)> = {
            if let Some(rw_lock) = self.spans.get(id_to_idx(&id)) {
                let mut span_data = rw_lock.write().unwrap();
                span_data.ref_ct -= 1; // decrement ref ct
                let ref_ct = span_data.ref_ct;

                drop(span_data);
                drop(rw_lock); // explicit drop to avoid deadlock on subsequent removal of this key from map

                if ref_ct == 0 {
                    // gen trace id _must_ be run before removing node from map b/c it looks up node.. mild wart
                    let trace_id = self.get_or_gen_trace_id(&id);
                    self.spans
                        .take(id_to_idx(&id))
                        .map(move |rw| (rw.into_inner().unwrap().inner, trace_id))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some((dropped, trace_id)) = dropped_span {
            let now = Utc::now();
            let now = now.timestamp_millis();
            let init_at = dropped.initialized_at.timestamp_millis();
            let elapsed_ms = now - init_at;

            let span = dropped.into_span(
                elapsed_ms,
                &self.service_name,
                self.instance_id,
                trace_id,
                id,
            );
            self.telemetry.report_span(span);
            true
        } else {
            false
        }
    }

    fn current_span(&self) -> Current {
        if let Some(id) = self.peek_current_span() {
            if let Some(meta) = self
                .spans
                .get(id_to_idx(&id))
                .map(|rw_lock| rw_lock.read().unwrap().metadata)
            {
                return Current::new(id, meta);
            }
        }
        Current::none()
    }
}

fn id_to_idx(id: &Id) -> usize {
    let idx = id.into_u64() as usize;
    idx - 1
}

fn idx_to_id(idx: usize) -> Id {
    let id = idx as u64;
    Id::from_u64(id + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::runtime::current_thread::Runtime;
    use tracing::instrument;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::Layer;

    #[test]
    fn test_instrument() {
        with_test_scenario_runner(|| {
            #[instrument]
            fn f(ns: Vec<u64>) {
                let explicit_trace_id = TraceId::new("test-trace-id".to_string());
                explicit_trace_id.record_on_current_span();
                for n in ns {
                    g(format!("{}", n));
                }
            }

            #[instrument]
            fn g(_s: String) {
                let use_of_reserved_word = "timestamp-value";
                tracing::event!(
                    tracing::Level::INFO,
                    timestamp = use_of_reserved_word,
                    foo = "bar"
                );
            }

            f(vec![1, 2, 3]);
        });
    }

    // run async fn (with multiple entry and exit for each span due to delay) with test scenario
    #[test]
    fn test_async_instrument() {
        with_test_scenario_runner(|| {
            #[instrument]
            async fn f(ns: Vec<u64>) {
                let explicit_trace_id = TraceId::new("test-trace-id".to_string());
                explicit_trace_id.record_on_current_span();
                for n in ns {
                    g(format!("{}", n)).await;
                }
            }

            #[instrument]
            async fn g(s: String) {
                // delay to force multiple span entry (because it isn't immediately ready)
                tokio::timer::delay_for(Duration::from_millis(100)).await;
                let use_of_reserved_word = "timestamp-value";
                tracing::event!(
                    tracing::Level::INFO,
                    timestamp = use_of_reserved_word,
                    foo = "bar"
                );
            }

            let mut rt = Runtime::new().unwrap();
            rt.block_on(f(vec![1, 2, 3]));
        });
    }

    fn with_test_scenario_runner<F>(f: F)
    where
        F: Fn() -> (),
    {
        let spans = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(Vec::new()));
        let cap = crate::telemetry::test::TestTelemetry::new(spans.clone(), events.clone());
        let subscriber = TelemetrySubscriber::new_("test_svc_name".to_string(), Box::new(cap));

        // filter out tracing noise
        let subscriber = LevelFilter::INFO.with_subscriber(subscriber);

        tracing::subscriber::with_default(subscriber, f);

        let spans = spans.lock().unwrap();
        let events = events.lock().unwrap();

        // root span is exited (and reported) last
        let root_span = &spans[3];
        let child_spans = &spans[0..3];

        fn expected(k: String, v: libhoney::Value) -> HashMap<String, libhoney::Value> {
            let mut h = HashMap::new();
            h.insert(k, v);
            h
        }

        let expected_trace_id = TraceId::new("test-trace-id".to_string());

        assert_eq!(
            root_span.values,
            expected("ns".to_string(), libhoney::json!("[1, 2, 3]"))
        );
        assert_eq!(root_span.parent_id, None);
        assert_eq!(root_span.trace_id, expected_trace_id);

        for (span, event) in child_spans.iter().zip(events.iter()) {
            // confirm parent and trace ids are as expected
            assert_eq!(span.parent_id, Some(root_span.id.clone()));
            assert_eq!(event.parent_id, Some(span.id.clone()));
            assert_eq!(span.trace_id, expected_trace_id);
            assert_eq!(event.trace_id, expected_trace_id);

            // test that reserved word field names are modified
            // (field names like "trace.span_id", "timestamp", etc are ok)
            assert_eq!(
                event.values["tracing.timestamp"],
                libhoney::json!("timestamp-value")
            )
        }
    }
}
