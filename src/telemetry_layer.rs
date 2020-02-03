use crate::telemetry::Telemetry;
use crate::trace::{self, SpanId, TraceCtx};
use chrono::{DateTime, Utc};
use rand::Rng;
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, registry, Layer};
use std::any::TypeId;
use std::default::Default;

/// Tracing 'Layer' that uses some telemetry client 'T' to publish events and spans
pub struct TelemetryLayer<T> {
    telemetry: T,
    service_name: String,
    // used to construct span ids to avoid collisions
    span_data: TraceCtxRegistry,
}

// resolvable via downcast_ref, to avoid propagating 'T' parameter of TelemetryLayer where not req'd
pub(crate) struct TraceCtxRegistry{
    registry: RwLock<HashMap<Id, TraceCtx>>,
    pub(crate) instance_id: u64,
}

impl TraceCtxRegistry {
    pub(crate) fn record_trace_ctx(&self, trace_ctx: TraceCtx, id: Id) {
        let mut span_data = self.registry.write().expect("write lock!");
        span_data.insert(id, trace_ctx); // TODO: handle overwrite?
    }

    pub(crate) fn eval_ctx<
        'a,
        X: 'a + registry::LookupSpan<'a>,
        I: std::iter::Iterator<Item = registry::SpanRef<'a, X>>,
    >(
        &self,
        iter: I,
    ) -> Option<TraceCtx> {
        let mut path = Vec::new();

        for span_ref in iter {
            let mut write_guard = span_ref.extensions_mut();
            match write_guard.get_mut() {
                None => {
                    let span_data = self.registry.read().unwrap();
                    match span_data.get(&span_ref.id()) {
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
                                write_guard.insert(LazyTraceCtx(TraceCtx {
                                    trace_id: local_trace_root.trace_id.clone(),
                                    parent_span: None,
                                }));
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
                        write_guard.insert(LazyTraceCtx(TraceCtx {
                            trace_id: already_evaluated.trace_id.clone(),
                            parent_span: None,
                        }));
                    }
                    return Some(res);
                }
            }
        }

        None
    }

    pub(crate) fn span_id(&self, tracing_id: Id) -> SpanId {
        SpanId {
            tracing_id,
            instance_id: self.instance_id,
        }
    }

    pub fn new() -> Self {
        let instance_id = rand::thread_rng().gen();
        let registry = RwLock::new(HashMap::new());

        TraceCtxRegistry {
            instance_id,
            registry,
        }
    }
}

impl<T> TelemetryLayer<T> {
    pub fn new(
        service_name: String,
        telemetry: T,
    ) -> Self {
        let span_data = TraceCtxRegistry::new();

        TelemetryLayer {
            service_name,
            telemetry,
            span_data,
        }
    }
}

impl<S, V: tracing::field::Visit + Default + Send + Sync + 'static, T: 'static + Telemetry<Visitor = V>> Layer<S> for TelemetryLayer<T>
where
    S: Subscriber + for<'a> registry::LookupSpan<'a>,
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
                if let Some(parent_trace_ctx) = self.span_data.eval_ctx(iter) {
                    let event = trace::Event {
                        trace_id: parent_trace_ctx.trace_id,
                        parent_id: Some(self.span_data.span_id(parent_id.clone())),
                        initialized_at,
                        level: event.metadata().level().clone(),
                        name: event.metadata().name(),
                        target: event.metadata().target(),
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

        // TODO: dedup
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
        if let Some(trace_ctx) = self.span_data.eval_ctx(iter) {
            let mut extensions_mut = span.extensions_mut();
            let visitor: V = extensions_mut
                .remove()
                .expect("should be present on all spans");
            let SpanInitAt(initialized_at) = extensions_mut
                .remove()
                .expect("should be present on all spans");

            let now = Utc::now();
            let now = now.timestamp_millis();
            let elapsed_ms = now - initialized_at.timestamp_millis();

            let parent_id = match trace_ctx.parent_span {
                None => span
                    .parents()
                    .next()
                    .map(|parent| self.span_data.span_id(parent.id())),
                Some(parent_span) => Some(parent_span),
            };

            let span = trace::Span {
                id: self.span_data.span_id(id),
                target: span.metadata().target(),
                level: span.metadata().level().clone(), // copy on inner type
                parent_id,
                name: span.metadata().name(),
                initialized_at: initialized_at.clone(),
                trace_id: trace_ctx.trace_id,
                elapsed_ms,
                service_name: &self.service_name,
                values: visitor,
            };

            self.telemetry.report_span(span);
        };
    }

    // FIXME: do I need to do something here? I think no (better to require explicit re-marking as root after copy).
    // called when span copied, needed iff span has trace id/etc already? nah,
    // fn on_id_change(&self, _old: &Id, _new: &Id, _ctx: Context<'_, S>) {}

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        println!("begin downcast raw");
        // This `downcast_raw` impl allows downcasting this layer to any of
        // its components (currently just trace ctx registry)
        // as well as to the layer's type itself.
        let res = match () {
            _ if id == TypeId::of::<Self>() => Some(self as *const Self as *const ()),
            _ if id == TypeId::of::<TraceCtxRegistry>() => Some(&self.span_data as *const TraceCtxRegistry as *const ()),
            _ => None,
        };

        println!("end downcast raw");

        res
    }
}

struct LazyTraceCtx(TraceCtx);

struct SpanInitAt(DateTime<Utc>);

impl SpanInitAt {
    fn new() -> Self {
        let initialized_at = Utc::now();

        Self(initialized_at)
    }
}

#[derive(Debug)]
struct PathToRoot<'a, S> {
    registry: &'a S,
    next: Option<Id>,
}

impl<'a, S> Iterator for PathToRoot<'a, S>
where
    S: registry::LookupSpan<'a>,
{
    type Item = registry::SpanRef<'a, S>;
    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next.take()?;
        let span = self.registry.span(&id)?;
        self.next = span.parent().map(|parent| parent.id());
        Some(span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::TraceId;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::runtime::Runtime;
    use tracing::instrument;
    use tracing_subscriber::layer::Layer;
    use crate::telemetry::test::TestTelemetry;

    fn explicit_trace_ctx() -> TraceCtx {
        let trace_id = TraceId::new("test-trace-id".to_string());
        let span_id = SpanId {
            tracing_id: Id::from_u64(1234),
            instance_id: 5678,
        };

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
                explicit_trace_ctx().record_on_current_span().unwrap();
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
                    TraceCtx::eval_current_trace_ctx()
                        .map(|x| x.trace_id)
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
                explicit_trace_ctx().record_on_current_span().unwrap();
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
                    TraceCtx::eval_current_trace_ctx()
                        .map(|x| x.trace_id)
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
        let cap: TestTelemetry<crate::telemetry::BlackholeVisitor> = TestTelemetry::new(spans.clone(), events.clone());
        let layer = TelemetryLayer::new("test_svc_name".to_string(), cap);

        let subscriber = layer.with_subscriber(registry::Registry::default());
        tracing::subscriber::with_default(subscriber, f);

        let spans = spans.lock().unwrap();
        let events = events.lock().unwrap();

        // root span is exited (and reported) last
        let root_span = &spans[3];
        let child_spans = &spans[0..3];

        let expected_trace_id = TraceId::new("test-trace-id".to_string());

        assert_eq!(root_span.parent_id, explicit_trace_ctx().parent_span);
        assert_eq!(root_span.trace_id, expected_trace_id);

        for (span, event) in child_spans.iter().zip(events.iter()) {
            // confirm parent and trace ids are as expected
            assert_eq!(span.parent_id, Some(root_span.id.clone()));
            assert_eq!(event.parent_id, Some(span.id.clone()));
            assert_eq!(span.trace_id, explicit_trace_ctx().trace_id);
            assert_eq!(event.trace_id, explicit_trace_ctx().trace_id);

            // this is a honeycomb visitor specific test
            // test that reserved word field names are modified w/ tracing. prefix
            // (field names like "trace.span_id", "duration_ms", etc are ok)
            // assert_eq!(
            //     event.values["tracing.duration_ms"],
            //     libhoney::json!("duration-value")
            // )
        }
    }
}
