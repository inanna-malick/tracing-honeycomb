use crate::telemetry::Telemetry;
use crate::types::{RefCt, SpanData, TelemetryObject, TraceId};
use crate::visitor::HoneycombVisitor;
use ::libhoney::json;
use chashmap::CHashMap;
use chrono::Utc;
use rand::Rng;
use std::cell::RefCell;
use std::collections::HashMap;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};
use tracing_core::span::Current;

// used by this subscriber to track the current span
thread_local! {
    static CURRENT_SPAN: RefCell<Vec<Id>> = RefCell::new(vec!());
}

pub struct TelemetrySubscriber {
    telem: Telemetry,
    spans: CHashMap<Id, RefCt<SpanData>>,
    service_name: String,
}

impl TelemetrySubscriber {
    pub fn new(service_name: String, telem: Telemetry) -> Self {
        TelemetrySubscriber {
            spans: CHashMap::new(),
            service_name,
            telem,
        }
    }

    pub fn record_trace_id(&self, trace_id: TraceId) {
        if let Some(id) = self.peek_current_span() {
            if let Some(mut s) = self.spans.get_mut(&id) {
                // open questions:
                // - what if this node already has a trace id (currently overwrites, mb panic?)
                s.trace_id = Some(trace_id);
            }
        }
    }

    /// this function provides lazy initialization of trace ids (only generated when req'd to observe honeycomb event/span)
    /// when a span's trace id is requested, that span and any parent spans can have their trace id evaluated and saved
    /// this function maintains an explicit stack of write guards to ensure no invalid trace id hierarchies result
    fn get_or_gen_trace_id(&self, target_id: &Id) -> TraceId {
        let mut path: Vec<chashmap::WriteGuard<Id, RefCt<SpanData>>> = vec![];
        let mut id = target_id.clone();

        let trace_id: TraceId = loop {
            if let Some(mut span) = self.spans.get_mut(&id) {
                if let Some(tid) = &span.trace_id {
                    // found already-eval'd trace id
                    break tid.clone();
                } else {
                    // span has no trace, must be updated as part of this call
                    if let Some(parent_id) = &span.parent_id {
                        id = parent_id.clone();
                    } else {
                        // found root span with no trace id, generate trace_id
                        let trace_id = TraceId::generate();
                        // subsequent break means we won't push span onto path so just update inline
                        span.trace_id = Some(trace_id.clone());
                        break trace_id;
                    };

                    path.push(span);
                };
            } else {
                // TODO: should I just panic if this happens?
                println!("did not expect this to happen - id deref fail during parent trace. generating trace id");
                break TraceId::generate();
            }
        };

        for mut span in path {
            span.trace_id = Some(trace_id.clone());
        }

        trace_id
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
    fn build_span<T: TelemetryObject>(&self, t: &T) -> (Id, SpanData) {
        let now = Utc::now();
        let mut u: u64 = 0;
        while u == 0 {
            u = rand::thread_rng().gen();
        } // random u64 != 0 required
        let id = Id::from_u64(u);

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

        (
            id,
            SpanData {
                initialized_at: now,
                metadata: t.t_metadata(),
                trace_id: None, // always lazy
                parent_id,
                values,
            },
        )
    }
}

impl Subscriber for TelemetrySubscriber {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() == &tracing::Level::INFO
            || metadata.level() == &tracing::Level::WARN
            || metadata.level() == &tracing::Level::ERROR
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        let (id, new_span) = self.build_span(span);

        // FIXME: what if span id already exists in map? should I handle? assume no overlap possible b/c random?
        // ASSERTION: there should be no collisions here
        // insert attributes from span into map
        self.spans.insert(
            id.clone(),
            RefCt {
                ref_ct: 1,
                inner: new_span,
            },
        );

        id
    }

    // record additional values on span map
    fn record(&self, span: &Id, values: &Record<'_>) {
        if let Some(mut span_data) = self.spans.get_mut(&span) {
            let mut visitor = HoneycombVisitor {
                accumulator: &mut span_data.values,
            };
            values.record(&mut visitor);
        } else {
            println!(
                "no span in map when recording to span with id {:?}, possible bug",
                span
            )
        }
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    // record event (publish directly to telemetry, not a span)
    fn event(&self, event: &Event<'_>) {
        // report as span with zero-length interval
        let (span_id, new_span) = self.build_span(event);

        // use parent trace id, if it exists
        let trace_id = new_span
            .parent_id
            .as_ref()
            .map(|pid| self.get_or_gen_trace_id(pid));

        let values = new_span.into_values(&self.service_name, trace_id, &span_id);

        self.telem.report_data(values);
    }

    fn enter(&self, span: &Id) {
        self.push_current_span(span.clone());
    }
    fn exit(&self, _span: &Id) {
        self.pop_current_span();
    }

    fn clone_span(&self, id: &Id) -> Id {
        if let Some(mut span_data) = self.spans.get_mut(id) {
            // should always be present
            span_data.ref_ct += 1;
        }
        id.clone() // type sig of this function seems to compel cloning of id (&X -> X)
    }

    fn try_close(&self, id: Id) -> bool {
        let dropped_span: Option<(SpanData, TraceId)> = {
            if let Some(mut span_data) = self.spans.get_mut(&id) {
                span_data.ref_ct -= 1; // decrement ref ct
                let ref_ct = span_data.ref_ct;
                drop(span_data); // explicit drop to avoid deadlock on subsequent removal of this key from map

                if ref_ct == 0 {
                    // IDEA: what if gen_trace_id _also_ does removal?
                    // IDEA: what if gen_trace_id is always run _post_ removal and is provided with a TelemetryObject that it consumes?
                    // TODO: ^^
                    // gen trace id _must_ be run before removing node from map b/c it uses lookup.. mild wart...
                    let trace_id = self.get_or_gen_trace_id(&id);
                    self.spans.remove(&id).map(move |e| (e.inner, trace_id))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some((dropped, trace_id)) = dropped_span {
            let now = Utc::now();
            let now = now.timestamp_subsec_millis();
            let init_at = dropped.initialized_at.timestamp_subsec_millis();
            let elapsed = now - init_at;

            let mut values = dropped.into_values(&self.service_name, Some(trace_id), &id);
            values.insert("duration_ms".to_string(), json!(elapsed));

            self.telem.report_data(values);
            true
        } else {
            false
        }
    }

    fn current_span(&self) -> Current {
        if let Some(id) = self.peek_current_span() {
            if let Some(meta) = self.spans.get(&id).map(|span| span.metadata) {
                return Current::new(id, meta);
            }
        }
        Current::none()
    }
}
