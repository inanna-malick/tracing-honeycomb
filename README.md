TODO: update links, move to leaf readmes

[![Documentation (master)](https://img.shields.io/badge/docs-master-brightgreen)](https://inanna-malick.github.io/honeycomb-tracing/honeycomb_tracing) [![Build Status](https://circleci.com/gh/inanna-malick/honeycomb-tracing/tree/master.svg?style=shield)](https://circleci.com/gh/inanna-malick/honeycomb-tracing/tree/master) [![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE-MIT)


TODO: heading, top level readme should just link to subpackages, subpackage readmes w/ content specific to each crate

bullet points for each, eg

# Workspace

This repo provides two crates:
- `tracing-distributed` contains generic machinery for publishing distributed trace telemetry to an arbitrary backend
- `tracing-honeycomb` contains a concrete implementation that uses honeycomb.io as a backend

As a tracing layer, `TelemetryLayer` can be composed with other layers to provide stdout logging, filtering, etc.


API SURFACE NOTE:
- doc comments to everything
- warn on missing docs (top of lib.rs)
- doc comments need periods (complete sentences)

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
