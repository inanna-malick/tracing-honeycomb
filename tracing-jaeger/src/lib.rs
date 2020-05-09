// #![deny(
//     warnings,
//     missing_debug_implementations,
//     missing_copy_implementations,
//     missing_docs
// )]

//! This crate provides:
//! - A tracing layer, `TelemetryLayer`, that can be used to publish trace data to honeycomb.io
//! - Utilities for implementing distributed tracing against the honeycomb.io backend
//!
//! As a tracing layer, `TelemetryLayer` can be composed with other layers to provide stdout logging, filtering, etc.

mod opentelemetry;
mod visitor;

pub use crate::opentelemetry::OpenTelemetry;
pub use crate::visitor::OpenTelemetryVisitor;
pub use ::opentelemetry::api::trace::span_context::{SpanId, TraceId};
use ::opentelemetry::exporter::trace::SpanExporter;
use ::opentelemetry::sdk::Config;
use rand::Rng;
use std::collections::HashMap;
use std::sync::Mutex;
#[doc(no_inline)]
pub use tracing_distributed::{TelemetryLayer, TraceCtxError};

/// Register the current span as the local root of a distributed trace.
///
/// Specialized to the opentelemetry-specific SpanId and TraceId provided by this crate.
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
/// Specialized to the opentelemetry-specific SpanId and TraceId provided by this crate.
pub fn current_dist_trace_ctx() -> Result<(TraceId, SpanId), TraceCtxError> {
    tracing_distributed::current_dist_trace_ctx()
}

/// Construct a TelemetryLayer that does not publish telemetry to any backend.
///
/// Specialized to the opentelemetry-specific SpanId and TraceId provided by this crate.
pub fn new_blackhole_telemetry_layer(
) -> TelemetryLayer<tracing_distributed::BlackholeTelemetry<SpanId, TraceId>, SpanId, TraceId> {
    TelemetryLayer::new(
        "opentelemetry_blackhole_tracing_layer",
        tracing_distributed::BlackholeTelemetry::default(),
        |tracing_id| SpanId::from_u64(tracing_id.into_u64()),
    )
}

/// Construct a TelemetryLayer that publishes telemetry to honeycomb.io using the provided honeycomb config.
///
/// Specialized to the honeycomb.io-specific SpanId and TraceId provided by this crate.
pub fn new_opentelemetry_layer(
    service_name: &'static str,
    exporter: Box<dyn SpanExporter>,
    config: Config,
) -> TelemetryLayer<OpenTelemetry, SpanId, TraceId> {
    // used to keep nodes in a multiprocess scenario from generating the same sequence of span ids
    let r: u64 = rand::thread_rng().gen();
    TelemetryLayer::new(
        service_name,
        OpenTelemetry {
            exporter,
            events: Mutex::new(HashMap::new()),
            config,
        },
        move |tracing_id| SpanId::from_u64(tracing_id.into_u64() ^ r),
    )
}
