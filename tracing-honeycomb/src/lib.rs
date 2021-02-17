#![deny(
    warnings,
    missing_debug_implementations,
    missing_copy_implementations,
    missing_docs
)]

//! This crate provides:
//! - A tracing layer, `TelemetryLayer`, that can be used to publish trace data to honeycomb.io
//! - Utilities for implementing distributed tracing against the honeycomb.io backend
//!
//! As a tracing layer, `TelemetryLayer` can be composed with other layers to provide stdout logging, filtering, etc.

mod honeycomb;
mod span_id;
mod trace_id;
mod visitor;

pub use honeycomb::HoneycombTelemetry;
pub use span_id::SpanId;
pub use trace_id::TraceId;
#[doc(no_inline)]
pub use tracing_distributed::{TelemetryLayer, TraceCtxError};
pub use visitor::HoneycombVisitor;

pub(crate) mod deterministic_sampler;

/// Register the current span as the local root of a distributed trace.
///
/// Specialized to the honeycomb.io-specific SpanId and TraceId provided by this crate.
pub fn register_dist_tracing_root(
    trace_id: TraceId,
    remote_parent_span: Option<SpanId>,
) -> Result<(), TraceCtxError> {
    tracing_distributed::register_dist_tracing_root(trace_id, remote_parent_span)
}

/// Retrieve the distributed trace context associated with the current span.
///
/// Returns the `TraceId`, if any, that the current span is associated with along with
/// the `SpanId` belonging to the current span.
///
/// Specialized to the honeycomb.io-specific SpanId and TraceId provided by this crate.
pub fn current_dist_trace_ctx() -> Result<(TraceId, SpanId), TraceCtxError> {
    tracing_distributed::current_dist_trace_ctx()
}

/// Construct a TelemetryLayer that does not publish telemetry to any backend.
///
/// Specialized to the honeycomb.io-specific SpanId and TraceId provided by this crate.
pub fn new_blackhole_telemetry_layer(
) -> TelemetryLayer<tracing_distributed::BlackholeTelemetry<SpanId, TraceId>, SpanId, TraceId> {
    TelemetryLayer::new(
        "honeycomb_blackhole_tracing_layer",
        tracing_distributed::BlackholeTelemetry::default(),
        move |tracing_id| SpanId { tracing_id },
    )
}

/// Construct a TelemetryLayer that publishes telemetry to honeycomb.io using the provided honeycomb config.
///
/// Specialized to the honeycomb.io-specific SpanId and TraceId provided by this crate.
pub fn new_honeycomb_telemetry_layer(
    service_name: &'static str,
    honeycomb_config: libhoney::Config,
) -> TelemetryLayer<HoneycombTelemetry, SpanId, TraceId> {
    TelemetryLayer::new(
        service_name,
        HoneycombTelemetry::new(honeycomb_config, None),
        move |tracing_id| SpanId { tracing_id },
    )
}

/// Construct a TelemetryLayer that publishes telemetry to honeycomb.io using the
/// provided honeycomb config, and sample rate. This function differs from
/// `new_honeycomb_telemetry_layer` and the `sample_rate` on the
/// `libhoney::Config` there in an important way. `libhoney` samples `Event`
/// data, which is individual spans on each trace. This means that using the
/// sampling logic in libhoney may result in missing event data or incomplete
/// traces. Calling this function provides trace-level sampling, meaning sampling
/// decisions are based on a modulo of the traceID, and events in a single trace
/// will not be sampled differently. If the trace is sampled, then all spans
/// under it will be sent to honeycomb. If a trace is not sampled, no spans or
/// events under it will be sent. When using this trace-level sampling, the
/// `sample_rate` parameter on the `libhoney::Config` should be set to 1, which
/// is the default.
///
/// Specialized to the honeycomb.io-specific SpanId and TraceId provided by this crate.
pub fn new_honeycomb_telemetry_layer_with_trace_sampling(
    service_name: &'static str,
    honeycomb_config: libhoney::Config,
    sample_rate: u32,
) -> TelemetryLayer<HoneycombTelemetry, SpanId, TraceId> {
    TelemetryLayer::new(
        service_name,
        HoneycombTelemetry::new(honeycomb_config, Some(sample_rate)),
        move |tracing_id| SpanId { tracing_id },
    )
}
