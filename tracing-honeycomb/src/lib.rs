#![deny(
    warnings,
    missing_debug_implementations,
    missing_copy_implementations,
    missing_docs
)]

//! This crate provides:
//! - A tracing layer, `TelemetryLayer`, that can be used to publish trace data to [honeycomb.io][].
//! - Utilities for implementing distributed tracing against the honeycomb.io backend.
//!
//! As a tracing layer, `TelemetryLayer` can be composed with other layers to provide stdout logging, filtering, etc.
//!
//! ### Propagating distributed tracing metadata
//! 
//! This crate provides two functions for out of band interaction with the `TelemetryLayer`
//! - `register_dist_tracing_root` registers the current span as the local root of a distributed trace.
//! - `current_dist_trace_ctx` fetches the `TraceId` and `SpanId` associated with the current span.
//! 
//! Here's an example of how they might be used together:
//! 1. Some span is registered as the global tracing root using a newly-generated `TraceId`.
//! 2. A child of that span uses `current_dist_trace_ctx` to fetch the current `TraceId` and `SpanId`. It passes these values along with an RPC request, as metadata.
//! 3. The RPC service handler uses the `TraceId` and remote parent `SpanId` provided in the request's metadata to register the handler function's span as a local root of the distributed trace initiated in step 1.
//! 
//! ### Registering a global Subscriber
//! 
//! The following example shows how to create and register a subscriber created by composing `TelemetryLayer` with other layers and the `Registry` subscriber provided by the `tracing_subscriber` crate.
//! 
//! ```no_run
//! use tracing_honeycomb::new_honeycomb_telemetry_layer;
//! use tracing_subscriber::prelude::*;
//! use tracing_subscriber::{filter::LevelFilter, fmt, registry::Registry};
//!
//! let honeycomb_config = libhoney::Config {
//!     options: libhoney::client::Options {
//!         api_key: std::env::var("HONEYCOMB_WRITEKEY").unwrap(),
//!         dataset: "my-dataset-name".to_string(),
//!         ..libhoney::client::Options::default()
//!     },
//!     transmission_options: libhoney::transmission::Options::default(),
//! };
//! 
//! let telemetry_layer = new_honeycomb_telemetry_layer("my-service-name", honeycomb_config);
//! 
//! // NOTE: the underlying subscriber MUST be the Registry subscriber
//! let subscriber = Registry::default() // provide underlying span data store
//!     .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
//!     .with(fmt::Layer::default()) // log to stdout
//!     .with(telemetry_layer); // publish to honeycomb backend
//! 
//! tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
//! ```
//! 
//! ### Testing
//! 
//! Since `TraceCtx::current_trace_ctx` and `TraceCtx::record_on_current_span` can be expected to return `Ok` as long as some `TelemetryLayer` has been registered as part of the layer/subscriber stack and the current span is active, it's valid to `.expect` them to always succeed & to panic if they do not. As a result, you may find yourself writing code that fails if no distributed tracing context is present. This means that unit and integration tests covering such code must provide a `TelemetryLayer`. However, you probably don't want to publish telemetry while running unit or integration tests. You can fix this problem by registering a `TelemetryLayer` constructed using `BlackholeTelemetry`. `BlackholeTelemetry` discards spans and events without publishing them to any backend.
//! 
//! ```
//! use tracing_honeycomb::new_blackhole_telemetry_layer;
//! use tracing_subscriber::prelude::*;
//! use tracing_subscriber::{filter::LevelFilter, fmt, registry::Registry};
//!
//! let telemetry_layer = new_blackhole_telemetry_layer(); 
//! 
//! // NOTE: the underlying subscriber MUST be the Registry subscriber
//! let subscriber = Registry::default() // provide underlying span data store
//!     .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
//!     .with(fmt::Layer::default()) // log to stdout
//!     .with(telemetry_layer); // publish to blackhole backend
//! 
//! tracing::subscriber::set_global_default(subscriber).ok();
//! ```
//!
//! [honeycomb.io]: https://www.honeycomb.io/


mod honeycomb;
mod visitor;

pub use crate::honeycomb::{HoneycombTelemetry, SpanId, TraceId};
pub use crate::visitor::HoneycombVisitor;
use rand::{self, Rng};
#[doc(no_inline)]
pub use tracing_distributed::{TelemetryLayer, TraceCtxError};

/// Register the current span as the local root of a distributed trace.
///
/// Specialized to the honeycomb.io-specific `SpanId` and `TraceId` provided by this crate.
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
/// Specialized to the honeycomb.io-specific `SpanId` and `TraceId` provided by this crate.
pub fn current_dist_trace_ctx() -> Result<(TraceId, SpanId), TraceCtxError> {
    tracing_distributed::current_dist_trace_ctx()
}

/// Construct a `TelemetryLayer` that does not publish telemetry to any backend.
///
/// Specialized to the honeycomb.io-specific `SpanId` and `TraceId` provided by this crate.
pub fn new_blackhole_telemetry_layer(
) -> TelemetryLayer<tracing_distributed::BlackholeTelemetry<SpanId, TraceId>, SpanId, TraceId> {
    let instance_id: u64 = 0;
    TelemetryLayer::new(
        "honeycomb_blackhole_tracing_layer",
        tracing_distributed::BlackholeTelemetry::default(),
        move |tracing_id| SpanId {
            instance_id,
            tracing_id,
        },
    )
}

/// Construct a `TelemetryLayer` that publishes telemetry to honeycomb.io using the provided honeycomb config.
///
/// Specialized to the honeycomb.io-specific `SpanId` and `TraceId` provided by this crate.
pub fn new_honeycomb_telemetry_layer(
    service_name: &'static str,
    honeycomb_config: libhoney::Config,
) -> TelemetryLayer<HoneycombTelemetry, SpanId, TraceId> {
    let instance_id: u64 = rand::thread_rng().gen();
    TelemetryLayer::new(
        service_name,
        HoneycombTelemetry::new(honeycomb_config, None),
        move |tracing_id| SpanId {
            instance_id,
            tracing_id,
        },
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
    sample_rate: u128,
) -> TelemetryLayer<HoneycombTelemetry, SpanId, TraceId> {
    let instance_id: u64 = rand::thread_rng().gen();
    TelemetryLayer::new(
        service_name,
        HoneycombTelemetry::new(honeycomb_config, Some(sample_rate)),
        move |tracing_id| SpanId {
            instance_id,
            tracing_id,
        },
    )
}
