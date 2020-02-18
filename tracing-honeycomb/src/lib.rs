#![deny(warnings)]
mod honeycomb;
mod visitor;

pub use crate::honeycomb::{HoneycombTelemetry, SpanId, TraceId};
pub use crate::visitor::HoneycombVisitor;
pub use tracing_distributed::{TelemetryLayer, TraceCtxError};

use rand::{self, Rng};

pub type TraceCtx = tracing_distributed::TraceCtx<SpanId, TraceId>;

pub fn current_dist_trace_ctx() -> Result<(TraceId, SpanId), TraceCtxError> {
    tracing_distributed::current_dist_trace_ctx()
}

pub fn mk_honeycomb_blackhole_tracing_layer(
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
