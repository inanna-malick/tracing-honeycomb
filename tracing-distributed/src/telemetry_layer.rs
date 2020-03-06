use crate::telemetry::Telemetry;
use crate::trace::{self, TraceCtx};
use chrono::{DateTime, Utc};
use std::any::TypeId;
use std::collections::HashMap;
use std::default::Default;
use std::sync::RwLock;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, registry, Layer};

/// Tracing 'Layer' that uses some telemetry client 'T' to publish events and spans
pub struct TelemetryLayer<Telemetry, SpanId, TraceId> {
    pub(crate) telemetry: Telemetry,
    service_name: &'static str,
    // used to construct span ids to avoid collisions
    pub(crate) trace_ctx_registry: TraceCtxRegistry<SpanId, TraceId>,
}

// resolvable via downcast_ref, to avoid propagating 'T' parameter of TelemetryLayer where not req'd
pub(crate) struct TraceCtxRegistry<SpanId, TraceId> {
    registry: RwLock<HashMap<Id, TraceCtx<SpanId, TraceId>>>,
    promote_span_id: Box<dyn 'static + Send + Sync + Fn(Id) -> SpanId>, // more of a haskell idiom, mb handle via some other means?
}

impl<SpanId, TraceId> TraceCtxRegistry<SpanId, TraceId>
where
    SpanId: 'static + Send + Clone + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    pub(crate) fn promote_span_id(&self, id: Id) -> SpanId {
        (self.promote_span_id)(id)
    }

    pub(crate) fn record_trace_ctx(&self, trace_ctx: TraceCtx<SpanId, TraceId>, id: Id) {
        let mut trace_ctx_registry = self.registry.write().expect("write lock!");
        trace_ctx_registry.insert(id, trace_ctx); // TODO: handle overwrite?
    }

    pub(crate) fn eval_ctx<
        'a,
        X: 'a + registry::LookupSpan<'a>,
        I: std::iter::Iterator<Item = registry::SpanRef<'a, X>>,
    >(
        &self,
        iter: I,
    ) -> Option<TraceCtx<SpanId, TraceId>> {
        let mut path = Vec::new();

        for span_ref in iter {
            let mut write_guard = span_ref.extensions_mut();
            match write_guard.get_mut::<LazyTraceCtx<SpanId, TraceId>>() {
                None => {
                    let trace_ctx_registry = self.registry.read().unwrap();
                    match trace_ctx_registry.get(&span_ref.id()) {
                        None => {
                            drop(write_guard);
                            path.push(span_ref);
                        }
                        Some(local_trace_root) => {
                            write_guard.insert(LazyTraceCtx(local_trace_root.clone()));

                            let res = if path.is_empty() {
                                local_trace_root.clone()
                            } else {
                                TraceCtx {
                                    trace_id: local_trace_root.trace_id.clone(),
                                    parent_span: None,
                                }
                            };

                            for span_ref in path.into_iter() {
                                let mut write_guard = span_ref.extensions_mut();
                                write_guard.insert::<LazyTraceCtx<SpanId, TraceId>>(LazyTraceCtx(
                                    TraceCtx {
                                        trace_id: local_trace_root.trace_id.clone(),
                                        parent_span: None,
                                    },
                                ));
                            }
                            return Some(res);
                        }
                    }
                }
                Some(LazyTraceCtx(already_evaluated)) => {
                    let res = if path.is_empty() {
                        already_evaluated.clone()
                    } else {
                        TraceCtx {
                            trace_id: already_evaluated.trace_id.clone(),
                            parent_span: None,
                        }
                    };

                    for span_ref in path.into_iter() {
                        let mut write_guard = span_ref.extensions_mut();
                        write_guard.insert::<LazyTraceCtx<SpanId, TraceId>>(LazyTraceCtx(
                            TraceCtx {
                                trace_id: already_evaluated.trace_id.clone(),
                                parent_span: None,
                            },
                        ));
                    }
                    return Some(res);
                }
            }
        }

        None
    }

    pub fn new<F: 'static + Send + Sync + Fn(Id) -> SpanId>(f: F) -> Self {
        let registry = RwLock::new(HashMap::new());
        let promote_span_id = Box::new(f);

        TraceCtxRegistry {
            registry,
            promote_span_id,
        }
    }
}

impl<T, SpanId, TraceId> TelemetryLayer<T, SpanId, TraceId>
where
    SpanId: 'static + Send + Clone + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    pub fn new<F: 'static + Send + Sync + Fn(Id) -> SpanId>(
        service_name: &'static str,
        telemetry: T,
        promote_span_id: F,
    ) -> Self {
        let trace_ctx_registry = TraceCtxRegistry::new(promote_span_id);

        TelemetryLayer {
            service_name,
            telemetry,
            trace_ctx_registry,
        }
    }
}

impl<S, TraceId, SpanId, V, T> Layer<S> for TelemetryLayer<T, SpanId, TraceId>
where
    S: Subscriber + for<'a> registry::LookupSpan<'a>,
    TraceId: 'static + Clone + Eq + Send + Sync,
    SpanId: 'static + Clone + Eq + Send + Sync,
    V: tracing::field::Visit + Default + Send + Sync + 'static,
    T: 'static + Telemetry<Visitor = V, TraceId = TraceId, SpanId = SpanId>,
{
    fn new_span(&self, attrs: &Attributes, id: &Id, ctx: Context<S>) {
        let span = ctx.span(id).expect("span data not found during new_span");
        let mut extensions_mut = span.extensions_mut();
        extensions_mut.insert(SpanInitAt::new());

        let mut visitor: V = Default::default();
        attrs.record(&mut visitor);
        extensions_mut.insert::<V>(visitor);
    }

    fn on_record(&self, id: &Id, values: &Record, ctx: Context<S>) {
        let span = ctx.span(id).expect("span data not found during on_record");
        let mut extensions_mut = span.extensions_mut();
        let visitor: &mut V = extensions_mut
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

                let mut visitor = Default::default();
                event.record(&mut visitor);

                // TODO: dedup
                let iter = itertools::unfold(Some(parent_id.clone()), |st| match st {
                    Some(target_id) => {
                        let res = ctx
                            .span(target_id)
                            .expect("span data not found during eval_ctx");
                        *st = res.parent().map(|x| x.id());
                        Some(res)
                    }
                    None => None,
                });

                // only report event if it's part of a trace
                if let Some(parent_trace_ctx) = self.trace_ctx_registry.eval_ctx(iter) {
                    let event = trace::Event {
                        trace_id: parent_trace_ctx.trace_id,
                        parent_id: Some(self.trace_ctx_registry.promote_span_id(parent_id)),
                        initialized_at,
                        meta: event.metadata(),
                        service_name: &self.service_name,
                        values: visitor,
                    };

                    self.telemetry.report_event(event);
                }
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("span data not found during on_close");

        // TODO: could be span.parents() but also needs span itself
        let iter = itertools::unfold(Some(id.clone()), |st| match st {
            Some(target_id) => {
                let res = ctx
                    .span(target_id)
                    .expect("span data not found during eval_ctx");
                *st = res.parent().map(|x| x.id());
                Some(res)
            }
            None => None,
        });

        // if span's enclosing ctx has a trace id, eval & use to report telemetry
        if let Some(trace_ctx) = self.trace_ctx_registry.eval_ctx(iter) {
            let mut extensions_mut = span.extensions_mut();
            let visitor: V = extensions_mut
                .remove()
                .expect("should be present on all spans");
            let SpanInitAt(initialized_at) = extensions_mut
                .remove()
                .expect("should be present on all spans");

            let now = Utc::now();
            let elapsed = now.signed_duration_since(initialized_at);

            let parent_id = match trace_ctx.parent_span {
                None => span
                    .parent()
                    .map(|parent_ref| self.trace_ctx_registry.promote_span_id(parent_ref.id())),
                Some(parent_span) => Some(parent_span),
            };

            let span = trace::Span {
                id: self.trace_ctx_registry.promote_span_id(id),
                meta: span.metadata(),
                parent_id,
                initialized_at,
                trace_id: trace_ctx.trace_id,
                elapsed,
                service_name: self.service_name,
                values: visitor,
            };

            self.telemetry.report_span(span);
        };
    }

    // FIXME: do I need to do something here? I think no (better to require explicit re-marking as root after copy).
    // called when span copied, needed iff span has trace id/etc already? nah,
    // fn on_id_change(&self, _old: &Id, _new: &Id, _ctx: Context<'_, S>) {}

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        // This `downcast_raw` impl allows downcasting this layer to any of
        // its components (currently just trace ctx registry)
        // as well as to the layer's type itself (technique borrowed from formatting subscriber)
        match () {
            _ if id == TypeId::of::<Self>() => Some(self as *const Self as *const ()),
            _ if id == TypeId::of::<TraceCtxRegistry<SpanId, TraceId>>() => Some(
                &self.trace_ctx_registry as *const TraceCtxRegistry<SpanId, TraceId> as *const (),
            ),
            _ => None,
        }
    }
}

struct LazyTraceCtx<SpanId, TraceId>(TraceCtx<SpanId, TraceId>);

struct SpanInitAt(DateTime<Utc>);

impl SpanInitAt {
    fn new() -> Self {
        let initialized_at = Utc::now();

        Self(initialized_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::test::{SpanId, TestTelemetry, TraceId};
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::runtime::Runtime;
    use tracing::instrument;
    use tracing_subscriber::layer::Layer;

    fn explicit_trace_ctx() -> TraceCtx<SpanId, TraceId> {
        let trace_id = 135;
        let span_id = Id::from_u64(246);

        TraceCtx {
            trace_id,
            parent_span: Some(span_id),
        }
    }

    #[test]
    fn test_instrument() {
        with_test_scenario_runner(|| {
            #[instrument]
            fn f(ns: Vec<u64>) {
                explicit_trace_ctx().register_dist_tracing_root().unwrap();
                for n in ns {
                    g(format!("{}", n));
                }
            }

            #[instrument]
            fn g(_s: String) {
                let use_of_reserved_word = "duration-value";
                tracing::event!(
                    tracing::Level::INFO,
                    duration_ms = use_of_reserved_word,
                    foo = "bar"
                );

                assert_eq!(
                    trace::current_dist_trace_ctx::<SpanId, TraceId>()
                        .map(|x| x.0)
                        .unwrap(),
                    explicit_trace_ctx().trace_id
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
                explicit_trace_ctx().register_dist_tracing_root().unwrap();
                for n in ns {
                    g(format!("{}", n)).await;
                }
            }

            #[instrument]
            async fn g(s: String) {
                // delay to force multiple span entry (because it isn't immediately ready)
                tokio::time::delay_for(Duration::from_millis(100)).await;
                let use_of_reserved_word = "duration-value";
                tracing::event!(
                    tracing::Level::INFO,
                    duration_ms = use_of_reserved_word,
                    foo = "bar"
                );

                assert_eq!(
                    trace::current_dist_trace_ctx::<SpanId, TraceId>()
                        .map(|x| x.0)
                        .unwrap(),
                    explicit_trace_ctx().trace_id
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
        let cap: TestTelemetry = TestTelemetry::new(spans.clone(), events.clone());
        let layer = TelemetryLayer::new("test_svc_name", cap, |x| x);

        let subscriber = layer.with_subscriber(registry::Registry::default());
        tracing::subscriber::with_default(subscriber, f);

        let spans = spans.lock().unwrap();
        let events = events.lock().unwrap();

        // root span is exited (and reported) last
        let root_span = &spans[3];
        let child_spans = &spans[0..3];

        let expected_trace_id = explicit_trace_ctx().trace_id;

        assert_eq!(root_span.parent_id, explicit_trace_ctx().parent_span);
        assert_eq!(root_span.trace_id, expected_trace_id);

        for (span, event) in child_spans.iter().zip(events.iter()) {
            // confirm parent and trace ids are as expected
            assert_eq!(span.parent_id, Some(root_span.id.clone()));
            assert_eq!(event.parent_id, Some(span.id.clone()));
            assert_eq!(span.trace_id, explicit_trace_ctx().trace_id);
            assert_eq!(event.trace_id, explicit_trace_ctx().trace_id);
        }
    }
}
