use crate::telemetry_layer::TraceCtxRegistry;
use tracing_subscriber::registry::LookupSpan;
use std::time::SystemTime;

/// Register the current span as the local root of a distributed trace.
pub fn register_dist_tracing_root<SpanId, TraceId>(
    trace_id: TraceId,
    remote_parent_span: Option<SpanId>,
) -> Result<(), TraceCtxError>
where
    SpanId: 'static + Clone + Send + Sync,
    TraceId: 'static + Clone + Send + Sync,
{
    let span = tracing::Span::current();
    span.with_subscriber(|(current_span_id, dispatch)| {
        if let Some(trace_ctx_registry) =
            dispatch.downcast_ref::<TraceCtxRegistry<SpanId, TraceId>>()
        {
            trace_ctx_registry.record_trace_ctx(
                trace_id,
                remote_parent_span,
                current_span_id.clone(),
            );
            Ok(())
        } else {
            Err(TraceCtxError::TelemetryLayerNotRegistered)
        }
    })
    .ok_or(TraceCtxError::NoEnabledSpan)?
}

/// Retrieve the distributed trace context associated with the current span. Returns the
/// `TraceId`, if any, that the current span is associated with along with the `SpanId`
/// belonging to the current span.
pub fn current_dist_trace_ctx<SpanId, TraceId>() -> Result<(TraceId, SpanId), TraceCtxError>
where
    SpanId: 'static + Clone + Send + Sync,
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

/// Errors that can occur while registering the current span as a distributed trace root or
/// attempting to retrieve the current trace context.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
#[non_exhaustive]
pub enum TraceCtxError {
    /// Expected a `TelemetryLayer` to be registered as a subscriber associated with the current Span.
    TelemetryLayerNotRegistered,
    /// Expected a `tracing_subscriber::Registry` to be registered as a subscriber associated with the current Span.
    RegistrySubscriberNotRegistered,
    /// Expected the span returned by `tracing::Span::current()` to be enabled, with an associated subscriber.
    NoEnabledSpan,
    /// Attempted to evaluate the current distributed trace context but none was found. If this occurs, you should check to make sure that `register_dist_tracing_root` is called in some parent of the current span.
    NoParentNodeHasTraceCtx,
}

/// A `Span` holds ready-to-publish information gathered during the lifetime of a `tracing::Span`.
#[derive(Debug, Clone)]
pub struct Span<Visitor, SpanId, TraceId> {
    /// id identifying this span
    pub id: SpanId,
    /// `TraceId` identifying the trace to which this span belongs
    pub trace_id: TraceId,
    /// optional parent span id
    pub parent_id: Option<SpanId>,
    /// UTC time at which this span was initialized
    pub initialized_at: SystemTime,
    /// `chrono::Duration` elapsed between the time this span was initialized and the time it was completed
    pub completed_at: SystemTime,
    /// `tracing::Metadata` for this span
    pub meta: &'static tracing::Metadata<'static>,
    /// name of the service on which this span occured
    pub service_name: &'static str,
    /// values accumulated by visiting fields observed by the `tracing::Span` this span was derived from
    pub values: Visitor,
}

/// An `Event` holds ready-to-publish information derived from a `tracing::Event`.
#[derive(Clone, Debug)]
pub struct Event<Visitor, SpanId, TraceId> {
    /// `TraceId` identifying the trace to which this event belongs
    pub trace_id: TraceId,
    /// optional parent span id
    pub parent_id: Option<SpanId>,
    /// UTC time at which this event was initialized
    pub initialized_at: SystemTime,
    /// `tracing::Metadata` for this event
    pub meta: &'static tracing::Metadata<'static>,
    /// name of the service on which this event occured
    pub service_name: &'static str,
    /// values accumulated by visiting the fields of the `tracing::Event` this event was derived from
    pub values: Visitor,
}
