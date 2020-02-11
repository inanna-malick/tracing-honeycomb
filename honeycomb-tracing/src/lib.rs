#![deny(warnings)]
mod telemetry;
mod visitor;

pub use crate::telemetry::{HoneycombTelemetry, SpanId, TraceId};
pub use crate::visitor::HoneycombVisitor;
pub use dist_tracing::{TelemetryLayer, TraceCtxError};

use rand::{self, Rng};

pub type TraceCtx = dist_tracing::TraceCtx<SpanId, TraceId>;

pub fn current_dist_trace_ctx() -> Result<(TraceId, SpanId), TraceCtxError> {
    dist_tracing::current_dist_trace_ctx()
}

pub fn mk_honeycomb_blackhole_tracing_layer(
) -> TelemetryLayer<dist_tracing::BlackholeTelemetry<SpanId, TraceId>, SpanId, TraceId> {
    let instance_id: u64 = 0;
    TelemetryLayer::new(
        "honeycomb_blackhole_tracing_layer",
        dist_tracing::BlackholeTelemetry::default(),
        move |tracing_id| SpanId {
            instance_id,
            tracing_id,
        },
    )
}

pub fn mk_honeycomb_tracing_layer(
    service_name: &'static str,
    honeycomb_config: libhoney::Config,
) -> TelemetryLayer<HoneycombTelemetry, SpanId, TraceId> {
    let instance_id: u64 = rand::thread_rng().gen();
    TelemetryLayer::new(
        service_name,
        HoneycombTelemetry::new(honeycomb_config),
        move |tracing_id| SpanId {
            instance_id,
            tracing_id,
        },
    )
}
