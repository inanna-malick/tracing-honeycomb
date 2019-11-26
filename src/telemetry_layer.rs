use crate::telemetry::{self, HoneycombTelemetry, SpanId, Telemetry, TraceCtx};
use crate::visitor::HoneycombVisitor;
use chashmap::CHashMap;
use chrono::{DateTime, Utc};
use rand::Rng;
use std::collections::HashMap;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, registry, Layer};

/// Tracing Subscriber that uses a 'libhoney-rust' Honeycomb client to publish spans
pub struct TelemetryLayer {
    telemetry: Box<dyn Telemetry + Send + Sync + 'static>,
    service_name: String,
    // used to construct span ids to avoid collisions
    instance_id: u64,
    // lazy trace ctx + init time
    span_data: CHashMap<Id, TraceCtx>,
}

// // NOTE: plan B: (also lets me keep lazy concurrent etc tek, which I like)
// // NOTE: basic idea is to keep span_id -> (remote parent span, trace id) map here w/ rwlocks (so u can get lock on whole tree to root node when forcing eval)
// // NOTE: can call register_root_node(id), w/ no trace or parent, to register some location as root (would prevent debug wrapper from being seen as root w/o requiring filtering for rest of stack)
// fn register_trace_id(trace_id: TraceId, remote_parent_span: Option<u128>) {
//     // with subscriber, from dispatcher as previously
//     let target_span_id = dispatcher.current; // available via dispatcher;
//     let tracing_layer = dispatcher.subscriber.downcast;
//     tracing_layer.observe_trace_id()
// }

impl TelemetryLayer {
    /// Create a new TelemetrySubscriber that uses the provided service_name and
    /// a Honeycomb client initialized using the provided 'libhoney::Config'
    pub fn new(service_name: String, config: libhoney::Config) -> Self {
        let telemetry = Box::new(HoneycombTelemetry::new(config));
        Self::new_(service_name, telemetry)
    }

    pub(crate) fn new_(
        service_name: String,
        telemetry: Box<dyn Telemetry + Send + Sync + 'static>,
    ) -> Self {
        let instance_id = rand::thread_rng().gen();

        TelemetryLayer {
            instance_id,
            service_name,
            telemetry,
            span_data: CHashMap::new(),
        }
    }

    pub(crate) fn record_trace_ctx(&self, trace_ctx: TraceCtx, id: Id) {
        // TODO: drop lazy trace stuff in extensions - can build vec of ExtensionMut
        // update span data map with explicit trace ctx
        self.span_data.upsert(
            id,
            move || trace_ctx,
            |existing_trace_ctx| {
                // panic? could be (will only happen if bug), but doesn't need to kill entire process. idk <- FIXME
                eprintln!(
                    "attempting to register a trace ctx \
                     on a span which already has a trace ctx registered, no-op {:?}",
                    &existing_trace_ctx
                )
            },
        )
    }

    fn eval_ctx<S: Subscriber + for<'a> registry::LookupSpan<'a>>(
        &self,
        target_id: &Id,
        ctx: &Context<S>,
    ) -> Option<TraceCtx> {
        let target_span_ref: tracing_subscriber::registry::SpanRef<S> = ctx
            .span(target_id)
            .expect("span data not found during eval_ctx");

        let mut path = Vec::new();

        let mut target_write_guard = target_span_ref.extensions_mut();
        match target_write_guard.get_mut() {
            None => match self.span_data.get(&target_span_ref.id()) {
                None => {

                    // iterate over parents
                    for span_ref in target_span_ref.parents() {
                        let mut write_guard = span_ref.extensions_mut();
                        match write_guard.get_mut() {
                            None => match self.span_data.get(&span_ref.id()) {
                                None => {
                                    drop(write_guard);
                                    path.push(span_ref);
                                }
                                Some(local_trace_root) => {
                                    write_guard.insert(LazyTraceCtx(local_trace_root.clone()));
                                    for span_ref in path.into_iter() {
                                        let mut write_guard = span_ref.extensions_mut();
                                        write_guard.insert(LazyTraceCtx(TraceCtx {
                                            trace_id: local_trace_root.trace_id.clone(),
                                            remote_span_parent: None,
                                        }));
                                    };
                                    target_write_guard.insert(LazyTraceCtx(local_trace_root.clone()));
                                    return Some(local_trace_root.clone());
                                }
                            },
                            Some(LazyTraceCtx(already_evaluated)) => {
                                for span_ref in path.into_iter() {
                                    let mut write_guard = span_ref.extensions_mut();
                                    write_guard.insert(LazyTraceCtx(already_evaluated.clone()));
                                }
                                target_write_guard.insert(LazyTraceCtx(already_evaluated.clone()));
                                return Some(already_evaluated.clone());
                            }
                        }
                    }
                }
                Some(local_trace_root) => {
                    target_write_guard.insert(LazyTraceCtx(local_trace_root.clone()));
                    return Some(local_trace_root.clone());
                }
            },
            Some(LazyTraceCtx(already_evaluated)) => {
                return Some(already_evaluated.clone());
            }
        };

        None
    }
}

impl<S> Layer<S> for TelemetryLayer
where
    S: Subscriber + for<'a> registry::LookupSpan<'a>,
{
    fn new_span(&self, attrs: &Attributes, id: &Id, ctx: Context<S>) {
        let span = ctx.span(id).expect("span data not found during new_span");
        let mut extensions_mut = span.extensions_mut();
        extensions_mut.insert(SpanInitAt::new());

        let mut visitor: HoneycombVisitor = HoneycombVisitor(HashMap::new());
        attrs.record(&mut visitor);
        extensions_mut.insert::<HoneycombVisitor>(visitor);
    }

    fn on_record(&self, id: &Id, values: &Record, ctx: Context<S>) {
        let span = ctx.span(id).expect("span data not found during on_record");
        let mut extensions_mut = span.extensions_mut();
        let visitor: &mut HoneycombVisitor = extensions_mut
            .get_mut()
            .expect("fields extension not found during on_record");
        values.record(visitor);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let parent_id = if let Some(parent_id) = event.parent() {
            // explicit parent
            Some(parent_id.clone())
        } else if event.is_root() {
            // don't bother checking thread local if span is explicitly root according to this fn
            None
        } else if let Some(parent_id) = ctx.current_span().id() {
            // implicit parent from threadlocal ctx
            Some(parent_id.clone())
        } else {
            // no parent span, thus this is a root span
            None
        };

        match parent_id {
            None => {} // not part of a trace, don't bother recording via honeycomb
            Some(parent_id) => {
                let initialized_at = Utc::now();

                let mut visitor = HoneycombVisitor(HashMap::new());
                event.record(&mut visitor);

                // TODO: modify get_or_set to return None if it gets to end of parents iterator w/ no result (instead of generating!)

                // only report event if it's part of a trace
                if let Some(parent_trace_ctx) = self.eval_ctx(&parent_id, &ctx) {
                    let event = telemetry::Event {
                        trace_id: parent_trace_ctx.trace_id,
                        parent_id: Some(SpanId::from_id(parent_id.clone(), self.instance_id)),
                        initialized_at,
                        level: event.metadata().level().clone(),
                        name: event.metadata().name(),
                        target: event.metadata().target(),
                        service_name: &self.service_name,
                        values: visitor.0,
                    };

                    self.telemetry.report_event(event);
                }
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("span data not found during on_close");

        // if span's enclosing ctx has a trace id, eval & use to report telemetry
        if let Some(trace_ctx) = self.eval_ctx(&id, &ctx) {
            let mut extensions_mut = span.extensions_mut();
            let visitor: HoneycombVisitor = extensions_mut
                .remove()
                .expect("should be present on all spans");
            let SpanInitAt(initialized_at) = extensions_mut
                .remove()
                .expect("should be present on all spans");

            let now = Utc::now();
            let now = now.timestamp_millis();
            let elapsed_ms = now - initialized_at.timestamp_millis();

            let parent_id = match trace_ctx.remote_span_parent {
                None => {
                    span.parents().next().map( |parent| SpanId::from_id(parent.id(), self.instance_id))
                }
                Some(remote_span_parent) => {
                    Some(remote_span_parent)
                }
            };


            let span = telemetry::Span {
                id: SpanId::from_id(id, self.instance_id),
                target: span.metadata().target(),
                level: span.metadata().level().clone(), // copy on inner type
                parent_id,
                name: span.metadata().name(),
                initialized_at: initialized_at.clone(),
                trace_id: trace_ctx.trace_id,
                elapsed_ms,
                service_name: &self.service_name,
                values: visitor.0,
            };

            self.telemetry.report_span(span);
        };
    }

    // FIXME: do I need to do something here?
    // called when span copied, needed iff span has trace id/etc already? nah,
    // fn on_id_change(&self, _old: &Id, _new: &Id, _ctx: Context<'_, S>) {}
}

struct LazyTraceCtx(TraceCtx);

struct SpanInitAt(DateTime<Utc>);
//  TODO: drop all but root tags in extensions

impl SpanInitAt {
    fn new() -> Self {
        let initialized_at = Utc::now();

        Self(initialized_at)
    }
}
