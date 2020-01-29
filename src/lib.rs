// #![deny(warnings)]
mod telemetry;
mod telemetry_layer;
mod visitor;

#[cfg(test)]
#[macro_use]
#[cfg(test)]
extern crate lazy_static;

pub use crate::telemetry::{
    BlackholeTelemetry, HoneycombTelemetry, SpanId, Telemetry, TraceCtx, TraceId,
};
pub use crate::telemetry_layer::TelemetryLayer;

// TODO: export test mock in-memory instance for ~signalling~ (also refactor a bit, should prob. expose telem. type)
