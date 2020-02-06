// #![deny(warnings)]
mod telemetry;
mod telemetry_layer;
mod trace;
mod visitor;

#[cfg(test)]
#[macro_use]
#[cfg(test)]
extern crate lazy_static;

pub use crate::telemetry::{BlackholeTelemetry, HoneycombTelemetry, Telemetry};
pub use crate::telemetry_layer::TelemetryLayer;
pub use crate::trace::{SpanId, TraceCtx, TraceId};

// TODO: export test mock in-memory instance for ~signalling~ (also refactor a bit, should prob. expose telem. type)
