use honeycomb_tracing::TelemetrySubscriber;
use std::time::Duration;
use tokio::timer::delay_for;
use tracing::instrument;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Layer;

#[instrument]
async fn foo() {
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
    let honeycomb_key = "YOUR-KEY".to_string();

    let honeycomb_config = libhoney::Config {
        options: libhoney::client::Options {
            api_key: honeycomb_key,
            dataset: "your-dataset".to_string(), // todo rename
            ..libhoney::client::Options::default()
        },
        transmission_options: libhoney::transmission::Options {
            max_batch_size: 1,
            ..libhoney::transmission::Options::default()
        },
    };

    let subscriber = TelemetrySubscriber::new("service-name".to_string(), honeycomb_config);
    let subscriber = LevelFilter::INFO.with_subscriber(subscriber);

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");

    loop {
        foo().await;
        delay_for(Duration::from_secs(10)).await
    }
}
