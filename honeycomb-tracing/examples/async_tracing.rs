use honeycomb_tracing::{HoneycombTelemetry, TelemetryLayer, TraceCtx};
use std::env;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::delay_for;
use tracing::instrument;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::registry;

#[instrument]
async fn spawn_children(n: u32, process_name: String) {
    TraceCtx::new_root().record_on_current_span().unwrap();

    for _ in 0..n {
        spawn_child(&process_name).await;
    }
}

#[instrument]
async fn spawn_child(process_name: &str) {
    let current_trace_ctx = TraceCtx::eval_current_trace_ctx().unwrap();
    let child = Command::new(process_name)
        .arg(current_trace_ctx.to_string())
        .spawn();

    // Make sure our child succeeded in spawning and process the result
    let future = child.expect("failed to spawn");

    // Await until the future (and the command) completes
    future.await.expect("awaiting process failed");
}

#[instrument]
async fn run_leaf_fn(trace_ctx: TraceCtx) {
    trace_ctx.record_on_current_span().unwrap();

    tracing::info!("leaf fn");
    delay_for(Duration::from_millis(50)).await
}

fn register_global_subscriber() {
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
        HoneycombTelemetry::new(honeycomb_config),
    );

    let subscriber = telemetry_layer // publish to tracing
        .and_then(tracing_subscriber::fmt::Layer::builder().finish()) // log to stdout
        .and_then(LevelFilter::INFO) // omit low-level debug tracing (eg tokio executor)
        .with_subscriber(registry::Registry::default()); // provide underlying span data store

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
}

#[tokio::main]
async fn main() {
    // parse first two args (including 0th arg, to get current process name)
    let mut iter = env::args();
    let process_name = iter.next().expect("expected first arg to be process name");
    let parent_trace_ctx = iter.next().and_then(|x| TraceCtx::from_string(&x));

    register_global_subscriber();

    match parent_trace_ctx {
        None => {
            // no parent trace_ctx, spawn child processes
            spawn_children(5, process_name).await;
        }
        Some(parent_trace_ctx) => {
            // parent trace ctx present, run leaf fn
            run_leaf_fn(parent_trace_ctx).await;
        }
    }

    // janky, but delay seems to be required to ensure all traces are sent to honeycomb by libhoney
    delay_for(Duration::from_secs(180)).await
}
