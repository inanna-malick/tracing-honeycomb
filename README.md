[![Documentation (master)](https://img.shields.io/badge/docs-master-brightgreen)](https://pkinsky.github.io/honeycomb-tracing/honeycomb_tracing/) [![Build Status](https://circleci.com/gh/pkinsky/honeycomb-tracing/tree/master.svg?style=shield)](https://circleci.com/gh/pkinsky/honeycomb-tracing/tree/master) [![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE-MIT)

Provides `TelemetryLayer`, a composable `tracing` layer that publishes spans and events to honeycomb.io. Supports local and distributed traces over async and sync functions.

todo: inline screenshot of trace for this

```rust
#[instrument]
async fn foo() {
    TraceCtx::new().record_on_current_span().unwrap();

    for n in 1..3 {
        bar(n).await;
    }
}

#[instrument]
async fn bar(x: u64) {
    tracing::info!("log info for bar iteration: {}", x);
    delay_for(Duration::from_millis(50)).await;

    for n in 1..3 {
        baz(n).await;
    }
}

#[instrument]
async fn baz(x: u64) {
    tracing::info!("log info for baz iteration: {}", x);
    delay_for(Duration::from_millis(50)).await
}
```

TODO: must explain trace ctx struct before here

The tracing layer provided by this crate, `TelemetryLayer`, provides out-of-band functionality via which users of this library can interact with the tracing layer via two functions:
- `TraceCtx::eval_current_trace_ctx() -> Result<TraceCtx, TraceCtxError)` makes the current span's 'TraceCtx', if any, available outside of `tracing` contexts. For example, an application that makes RPC requests (eg, a GRPC client) might use this to include a 'TraceCtx' in the metadata of an RPC request.
- `TraceCtx::record_on_current_span(self) -> Result<(), TraceCtxError)` associates a 'TraceCtx' with the current span. For example, an application that handles RPC requests (eg, a GRPC server) might use it to mark some span as the root of a trace or to register a span as being part of a distributed trace by parsing a 'TraceCtx' from RPC metadata. 


## Examples

This layer uses libhoney to publish telemetry to honeycomb. As a tracing layer, it can be composed with other layers. However, the underlying subscriber must be `tracing_subscriber::registry::Registry`. The following example shows how to create and register a subscriber created by composing `TelemetryLayer` (as provided by this crate) with other layers and a subscriber.


```rust
    let honeycomb_config = libhoney::Config {
        options: libhoney::client::Options {
            api_key: honeycomb_key,
            dataset: "dag-cache".to_string(), // FIXME: rename if copying this example
            ..libhoney::client::Options::default()
        },
        transmission_options: libhoney::transmission::Options::default(),
    };

    let telemetry_layer = TelemetryLayer::new(
        "async-tracing-example".to_string(),
        Box::new(HoneycombTelemetry::new(honeycomb_config)),
    );

    let subscriber = telemetry_layer // publish to tracing
        .and_then(tracing_subscriber::fmt::Layer::builder().finish()) // log to stdout
        .and_then(LevelFilter::INFO) // omit low-level debug tracing (eg tokio executor)
        .with_subscriber(registry::Registry::default()); // provide underlying span data store

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");

```

### Testing

Since `eval_current_trace_ctx` and `record_on_current_span` can be expected to return `Ok` as long as `TelemetryLayer` has been registered as part of the layer/subscriber stack and the current span is active, it's normal to `.expect` them to always succeed & to panic if they do not. However, in tests you likely will not want to publish telemetry. This library provides a `BlackholeTelemetry` instance for this case, so you can have a layer of the correct type published & exercise any code that uses trace ctxs in tests. Use as:

```rust
    let telemetry_layer = TelemetryLayer::new(
        "foo".to_string(),
        Box::new(BlackholeTelemetry),
    );

    let subscriber = telemetry_layer // publish to tracing
        .and_then(tracing_subscriber::fmt::Layer::builder().finish()) // log to stdout
        .and_then(LevelFilter::INFO) // omit low-level debug tracing (eg tokio executor)
        .with_subscriber(registry::Registry::default()); // provide underlying span data store

    let subscriber = layer.with_subscriber(registry::Registry::default());

    // attempt to set, failure means already set (other test suite, likely)
    let _ = tracing::subscriber::set_global_default(subscriber);
```
