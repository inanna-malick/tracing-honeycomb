use std::{env, str::FromStr, time::Duration};
use tokio::process::Command;
use tokio::time::delay_for;
use tracing::instrument;
use tracing_honeycomb::{
    current_dist_trace_ctx, new_honeycomb_telemetry_layer, register_dist_tracing_root, SpanId,
    TraceId,
};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

#[instrument]
async fn spawn_children(n: u32, process_name: String) {
    register_dist_tracing_root(TraceId::new(), None).unwrap();

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
async fn run_in_child_process(trace_id: TraceId, parent_span: SpanId) {
    register_dist_tracing_root(trace_id, Some(parent_span)).unwrap();

    tracing::info!("leaf fn");
    delay_for(Duration::from_millis(50)).await
}

#[tokio::main]
async fn main() {
    // parse first two args (including 0th arg, to get current process name)
    let mut iter = env::args();
    let process_name = iter.next().expect("expected first arg to be process name");
    let parent_span = iter.next();
    let trace_id = iter.next();

    register_global_subscriber();

    match (parent_span, trace_id) {
        (Some(parent_span), Some(trace_id)) => {
            let parent_span = SpanId::from_str(&parent_span).unwrap();
            let trace_id = TraceId::from_str(&trace_id).unwrap();
            // parent trace ctx present, run leaf fn
            run_in_child_process(trace_id, parent_span).await;
        }
        _ => {
            // no parent trace_ctx, spawn child processes
            spawn_children(5, process_name).await;
        }
    }

    // janky, but delay seems to be required to ensure all traces are sent to honeycomb by libhoney
    delay_for(Duration::from_secs(10)).await
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

    let telemetry_layer = new_honeycomb_telemetry_layer("async-tracing_example", honeycomb_config);

    let subscriber = registry::Registry::default() // provide underlying span data store
        .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
        .with(tracing_subscriber::fmt::Layer::default()) // log to stdout
        .with(telemetry_layer); // publish to honeycomb backend

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
}
