use crate::telemetry_layer::TraceCtxRegistry;
use chrono::{DateTime, Utc};
use tracing_subscriber::registry::LookupSpan;

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceCtx<S, T> {
    pub parent_span: Option<S>,
    pub trace_id: T,
}

impl<SpanId, TraceId> TraceCtx<SpanId, TraceId>
where
    SpanId: 'static + Send + Clone + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    pub fn register_dist_tracing_root(self) -> Result<(), TraceCtxError> {
        let span = tracing::Span::current();
        span.with_subscriber(|(current_span_id, dispatch)| {
            if let Some(trace_ctx_registry) =
                dispatch.downcast_ref::<TraceCtxRegistry<SpanId, TraceId>>()
            {
                trace_ctx_registry.record_trace_ctx(self, current_span_id.clone());
                Ok(())
            } else {
                Err(TraceCtxError::TelemetryLayerNotRegistered)
            }
        })
        .ok_or(TraceCtxError::NoEnabledSpan)?
    }
}

// NOTE: doesn't return TraceCtx because, if successful, will always have parent span id (so no need for Option)
pub fn current_dist_trace_ctx<SpanId, TraceId>() -> Result<(TraceId, SpanId), TraceCtxError>
where
    SpanId: 'static + Send + Clone + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    let span = tracing::Span::current();
    span.with_subscriber(|(current_span_id, dispatch)| {
        let trace_ctx_registry = dispatch
            .downcast_ref::<TraceCtxRegistry<SpanId, TraceId>>()
            .ok_or(TraceCtxError::TelemetryLayerNotRegistered)?;

        let registry = dispatch
            .downcast_ref::<tracing_subscriber::Registry>()
            .ok_or(TraceCtxError::RegistrySubscriberNotRegistered)?;

        let iter = itertools::unfold(Some(current_span_id.clone()), |st| match st {
            Some(target_id) => {
                // failure here indicates a broken parent id span link, panic is valid
                let res = registry
                    .span(target_id)
                    .expect("span data not found during eval_ctx for current_trace_ctx");
                *st = res.parent().map(|x| x.id());
                Some(res)
            }
            None => None,
        });

        trace_ctx_registry
            .eval_ctx(iter)
            .map(|x| {
                (
                    x.trace_id,
                    trace_ctx_registry.promote_span_id(current_span_id.clone()),
                )
            })
            .ok_or(TraceCtxError::NoParentNodeHasTraceCtx)
    })
    .ok_or(TraceCtxError::NoEnabledSpan)?
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
#[non_exhaustive]
pub enum TraceCtxError {
    TelemetryLayerNotRegistered,
    RegistrySubscriberNotRegistered,
    NoEnabledSpan,
    NoParentNodeHasTraceCtx, // no parent node has explicitly registered trace ctx
}

#[derive(Debug, Clone)]
pub struct Span<V, S, T> {
    pub id: S,
    pub trace_id: T,
    pub parent_id: Option<S>,
    pub initialized_at: DateTime<Utc>,
    pub elapsed: chrono::Duration,
    pub meta: &'static tracing::Metadata<'static>,
    pub service_name: &'static str,
    pub values: V, // visitor used to record values
}

#[derive(Clone, Debug)]
pub struct Event<V, S, T> {
    pub trace_id: T,
    pub parent_id: Option<S>,
    pub initialized_at: DateTime<Utc>,
    pub meta: &'static tracing::Metadata<'static>,
    pub service_name: &'static str,
    pub values: V,
}
