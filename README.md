[![Documentation (master)](https://img.shields.io/badge/docs-master-brightgreen)](https://pkinsky.github.io/honeycomb-tracing/honeycomb_tracing/) [![Build Status](https://circleci.com/gh/pkinsky/honeycomb-tracing/tree/master.svg?style=shield)](https://circleci.com/gh/pkinsky/honeycomb-tracing/tree/master) [![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE-MIT)

tracing subscriber for use with honeycomb.io distributed tracing. Supports generating random trace IDs or recording known trace IDs on the current span.



```rust
let honeycomb_config = libhoney::Config {
    options: libhoney::client::Options {
        api_key: "MY-API-KEY",
        dataset: "my-dataset-name".to_string(),
        ..libhoney::client::Options::default()
    },
    transmission_options: libhoney::transmission::Options::default(),
};

let subscriber = TelemetrySubscriber::new("my-service-name".to_string(), honeycomb_config);
// filter out tracing noise
let subscriber = LevelFilter::INFO.with_subscriber(subscriber);
tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
```
