# tracing-distributed

This crate provides:
- `TelemetryLayer`, a generic tracing layer that handles publishing spans and events to arbitrary backends
- Utilities for implementing distributed tracing for arbitrary backends

As a tracing layer, `TelemetryLayer` can be composed with other layers to provide stdout logging, filtering, etc.

This crate is primarily intended to be used by people implementing their own backends. A concrete implementation using honeycomb.io as a backend is available at (TODO: link to tracing-honeycomb).

## Implementation note

`TelemetryLayer` relies on an instance of the `Telemetry` trait. Implementors of this this trait must provide three associated types:
- `TraceId`, a globally unique identifier that associates spans and events with a distributed trace
- `SpanId`, an identifier that uniquely identifies spans within a trace
- `Visitor`, a type that is used to record values associated with a given span or event.

 
