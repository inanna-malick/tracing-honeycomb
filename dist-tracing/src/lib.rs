#![deny(warnings)]
mod telemetry;
mod telemetry_layer;
mod trace;

#[cfg(test)]
#[macro_use]
#[cfg(test)]
extern crate lazy_static;

pub use crate::telemetry::{BlackholeTelemetry, Telemetry};
pub use crate::telemetry_layer::TelemetryLayer;
pub use crate::trace::{Event, Span, SpanId, TraceCtx, TraceId};
