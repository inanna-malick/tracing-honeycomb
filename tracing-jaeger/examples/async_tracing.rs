use rand::Rng;
use std::{env, str::FromStr, time::Duration};
use tokio::process::Command;
use tokio::time::delay_for;
use tracing::instrument;
use tracing_jaeger::{
    current_dist_trace_ctx, new_opentelemetry_layer, register_dist_tracing_root, SpanId, TraceId,
};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

#[instrument]
async fn spawn_children(n: u32, process_name: String) {
    let trace_id = rand::thread_rng().gen();
    register_dist_tracing_root(TraceId::from_u128(trace_id), None).unwrap();

    for _ in 0..n {
        spawn_child_process(&process_name).await;
    }
}

#[instrument]
async fn spawn_child_process(process_name: &str) {
    let (trace_id, span_id) = current_dist_trace_ctx().unwrap();
    let child = Command::new(process_name)
        .arg(span_id.to_u64().to_string())
        .arg(trace_id.to_u128().to_string())
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
            let parent_span = u64::from_str(&parent_span).map(SpanId::from_u64).unwrap();
            let trace_id = u128::from_str(&trace_id).map(TraceId::from_u128).unwrap();
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
    let exporter = opentelemetry_jaeger::Exporter::builder()
        .with_agent_endpoint("localhost:6831".parse().unwrap())
        .with_process(opentelemetry_jaeger::Process {
            service_name: "trace-demo".to_string(),
            tags: vec![],
        })
        .init()
        .unwrap();

    let telemetry_layer = new_opentelemetry_layer(
        "async-tracing_example",
        Box::new(exporter),
        Default::default(),
    );

    let subscriber = registry::Registry::default() // provide underlying span data store
        .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
        .with(tracing_subscriber::fmt::Layer::default()) // log to stdout
        .with(telemetry_layer); // publish to honeycomb backend

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");
}
