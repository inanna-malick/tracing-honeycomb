use honeycomb_tracing::{HoneycombTelemetry, TelemetryLayer, TraceCtx};
use std::time::Duration;
use tokio::time::delay_for;
use tracing::instrument;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::registry;

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

#[tokio::main]
async fn main() {
    let honeycomb_key = std::fs::read_to_string("honeycomb.key")
        .expect("example requires honeycomb.key file with your honeycomb key");

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

    loop {
        foo().await;
        delay_for(Duration::from_secs(180)).await
    }
}
