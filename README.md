[![Documentation (master)](https://img.shields.io/badge/docs-master-brightgreen)](https://inanna-malick.github.io/honeycomb-tracing/honeycomb_tracing) [![Build Status](https://circleci.com/gh/inanna-malick/honeycomb-tracing/tree/master.svg?style=shield)](https://circleci.com/gh/inanna-malick/honeycomb-tracing/tree/master) [![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE-MIT)

Provides `TelemetryLayer`, a composable `tracing` layer that publishes spans and events to honeycomb.io, with support for arbitrary backends. Also provides utilities for implementing distributed tracing for arbitrary backends, with a concrete implementation using honeycomb.io as a backend in `tracing-honeycomb`. Each implementation can provide its own `TraceId` and `SpanId` types, with the goal of allowing the underlying machinery provided by `tracing-distributed`.

As a tracing layer, `TelemetryLayer` can be composed with other layers to provide stdout logging, filtering, etc. However, the underlying subscriber must be `tracing_subscriber::registry::Registry`. The following example shows how to create and register a subscriber created by composing `TelemetryLayer` (as provided by this crate) with other layers and a registry subscriber. 

```rust
let honeycomb_config = libhoney::Config {
    options: libhoney::client::Options {
        api_key: honeycomb_key,
        dataset: "my-dataset-name".to_string(),
        ..libhoney::client::Options::default()
    },
    transmission_options: libhoney::transmission::Options::default(),
};

let telemetry_layer = mk_honeycomb_tracing_layer("my-service-name", honeycomb_config);

let subscriber = telemetry_layer // publish to tracing
    .and_then(tracing_subscriber::fmt::Layer::builder().finish()) // log events to stdout
    .and_then(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
    .with_subscriber(registry::Registry::default()); // provide underlying span data store

tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
```

A `TraceCtx` uniquely identifies the current trace via some `TraceId` and an optional parent `SpanId`. `TelemetryLayer` provides out-of-band functionality for recording and retreiving trace contexts:
- `TraceCtx::current_trace_ctx() -> Result<TraceCtx, TraceCtxError)` makes the current span's 'TraceCtx', if any, available outside of `tracing` contexts. For example, an application that makes RPC requests (eg, a GRPC client) might use this to include a 'TraceCtx' in the metadata of an RPC request, such that tracing info is propagated across RPC request boundaries.
- `TraceCtx::record_on_current_span(self) -> Result<(), TraceCtxError)` associates a 'TraceCtx' with the current span. For example, an application that handles RPC requests (eg, a GRPC server) might use it to mark some span as the root of a trace or to register a span as being part of a distributed trace by parsing a 'TraceCtx' from RPC metadata. 


## Examples

![example honeycomb.io trace](/images/example_trace.png)

see `honeycomb-tracing/examples/async-tracing` for a simple multiprocess example that spawns child processes and uses the functions described above to group them under a single trace.

```rust
#[instrument]
async fn spawn_children(n: u32, process_name: String) {
    TraceCtx::new_root().record_on_current_span().unwrap();

    for _ in 0..n {
        spawn_child_process(&process_name).await;
    }
}

#[instrument]
async fn spawn_child_process(process_name: &str) {
    let (trace_id, span_id) = current_dist_trace_ctx().unwrap();
    let child = Command::new(process_name)
        .arg(span_id.to_string())
        .arg(trace_id.to_string())
        .spawn();

    // Make sure our child succeeded in spawning and process the result
    let future = child.expect("failed to spawn");

    // Await until the future (and the command) completes
    future.await.expect("awaiting process failed");
}

#[instrument]
async fn run_in_child_process(trace_ctx: TraceCtx) {
    trace_ctx.record_on_current_span().unwrap();

    tracing::info!("leaf fn");
    delay_for(Duration::from_millis(50)).await
}

```

### Testing

Since `TraceCtx::current_trace_ctx` and `TraceCtx::record_on_current_span` can be expected to return `Ok` as long as some `TelemetryLayer` has been registered as part of the layer/subscriber stack and the current span is active, it's valid to `.expect` them to always succeed & to panic if they do not.

This library provides a `BlackholeTelemetry` `Telemetry` instance for use in test code, so you can exercise code that uses trace ctxs in tests without publishing telemetry to any backend. Use as:

```rust
let telemetry_layer = mk_honeycomb_blackhole_tracing_layer(); 

let subscriber = telemetry_layer
    .and_then(tracing_subscriber::fmt::Layer::builder().finish()) // log events to stdout
    .with_subscriber(registry::Registry::default()); // provide underlying span data store

tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
```
