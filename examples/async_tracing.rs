use honeycomb_tracing::TelemetryLayer;
use honeycomb_tracing::{TraceCtx, TraceId};
use std::time::Duration;
use tokio::timer::delay_for;
use tracing::instrument;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::registry;
use tracing_subscriber::filter::LevelFilter;

#[instrument]
async fn foo() {
    TraceCtx {
        trace_id: TraceId::generate(),
        parent_span: None,
    }
    .record_on_current_span();

    println!("foo");
    for n in 1..3 {
        baz(n).await;
    }
}

#[instrument]
async fn baz(x: u64) {
    println!("baz");
    tracing::info!("baz iteration: {}", x);
    delay_for(Duration::from_millis(50)).await
}

#[tokio::main]
async fn main() {
    let honeycomb_key =
        std::fs::read_to_string("honeycomb.key").expect("expected file honeycomb.key to exist");

    let honeycomb_config = libhoney::Config {
        options: libhoney::client::Options {
            api_key: honeycomb_key,
            dataset: "dag-cache".to_string(), // todo rename
            ..libhoney::client::Options::default()
        },
        transmission_options: libhoney::transmission::Options {
            max_batch_size: 1,
            ..libhoney::transmission::Options::default()
        },
    };

    let layer = TelemetryLayer::new("async-tracing-example".to_string(), honeycomb_config)
        .and_then(tracing_subscriber::fmt::Layer::builder().finish())
        .and_then(LevelFilter::INFO);

    let subscriber = layer.with_subscriber(registry::Registry::default());

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");

    loop {
        foo().await;
        delay_for(Duration::from_secs(6000)).await
    }
}
