[![tracing-honeycomb on crates.io](https://img.shields.io/crates/v/tracing-honeycomb)](https://crates.io/crates/tracing-honeycomb) [![Documentation (latest release)](https://docs.rs/tracing-honeycomb/badge.svg)](https://docs.rs/tracing-honeycomb/) [![Documentation (master)](https://img.shields.io/badge/docs-master-brightgreen)](https://inanna-malick.github.io/honeycomb-tracing/tracing_honeycomb/)[![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)[![inanna-malick](https://circleci.com/gh/inanna-malick/honeycomb-tracing.svg?style=svg)](https://app.circleci.com/pipelines/github/inanna-malick/honeycomb-tracing)

# {{crate}}

{{readme}}

## Usage

Add the following to your Cargo.toml to get started.

```toml
tracing-honeycomb = {{version}}
```

### Propagating distributed tracing metadata

This crate provides two functions for out of band interaction with the `TelemetryLayer`
- `register_dist_tracing_root` registers the current span as the local root of a distributed trace.
- `current_dist_trace_ctx` fetches the `TraceId` and `SpanId` associated with the current span.

Here's an example of how they might be used together:
1. Some span is registered as the global tracing root using a newly-generated `TraceId`.
2. A child of that span uses `current_dist_trace_ctx` to fetch the current `TraceId` and `SpanId`. It passes these values along with an RPC request, as metadata.
3. The RPC service handler uses the `TraceId` and remote parent `SpanId` provided in the request's metadata to register the handler function's span as a local root of the distributed trace initiated in step 1.

### Registering a global Subscriber

The following example shows how to create and register a subscriber created by composing `TelemetryLayer` with other layers and the `Registry` subscriber provided by the `tracing_subscriber` crate.

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

// NOTE: the underlying subscriber MUST be the Registry subscriber
let subscriber = registry::Registry::default() // provide underlying span data store
    .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
    .with(tracing_subscriber::fmt::Layer::builder().finish()) // log to stdout
    .with(telemetry_layer); // publish to honeycomb backend


tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
```

### Testing

Since `TraceCtx::current_trace_ctx` and `TraceCtx::record_on_current_span` can be expected to return `Ok` as long as some `TelemetryLayer` has been registered as part of the layer/subscriber stack and the current span is active, it's valid to `.expect` them to always succeed & to panic if they do not. As a result, you may find yourself writing code that fails if no distributed tracing context is present. This means that unit and integration tests covering such code must provide a `TelemetryLayer`. However, you probably don't want to publish telemetry while running unit or integration tests. You can fix this problem by registering a `TelemetryLayer` constructed using `BlackholeTelemetry`. `BlackholeTelemetry` discards spans and events without publishing them to any backend.

```rust
let telemetry_layer = mk_honeycomb_blackhole_tracing_layer(); 

// NOTE: the underlying subscriber MUST be the Registry subscriber
let subscriber = registry::Registry::default() // provide underlying span data store
    .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
    .with(tracing_subscriber::fmt::Layer::builder().finish()) // log to stdout
    .with(telemetry_layer); // publish to blackhole backend

tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
```

## License

{{license}}

<!--
README.md is generated from README.tpl by cargo readme. To regenerate:
cargo install cargo-readme
cargo readme > README.md
-->
