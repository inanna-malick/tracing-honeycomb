[![tracing-honeycomb on crates.io](https://img.shields.io/crates/v/tracing-honeycomb)](https://crates.io/crates/tracing-honeycomb) [![Documentation (latest release)](https://docs.rs/tracing-honeycomb/badge.svg)](https://docs.rs/tracing-honeycomb/) [![Documentation (master)](https://img.shields.io/badge/docs-master-brightgreen)](https://inanna-malick.github.io/tracing-honeycomb/tracing_honeycomb/) [![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE) [![CircleCI status](https://circleci.com/gh/inanna-malick/tracing-honeycomb.svg?style=svg)](https://app.circleci.com/pipelines/github/inanna-malick/tracing-honeycomb)

# tracing-honeycomb

This repo contains the source code for:
- [`tracing-distributed`](tracing-distributed/README.md), which contains generic machinery for publishing distributed trace telemetry to arbitrary backends.
- [`tracing-honeycomb`](tracing-honeycomb/README.md), which contains a concrete implementation that uses [honeycomb.io](https://honeycomb.io) as a backend.
- [`tracing-jaeger`](tracing-jaeger/README.md), which contains a concrete implementation that uses [jaegertracing.io](https://www.jaegertracing.io/) as a backend.

## Usage

See [`tracing-honeycomb`](tracing-honeycomb/README.md) for examples.

## License

This project is licensed under the terms of the MIT license

