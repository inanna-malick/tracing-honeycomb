# tracing-honeycomb

This repo contains the source code for:
- [`tracing-distributed`](tracing-distributed/README.md) contains generic machinery for publishing distributed trace telemetry to an arbitrary backend
- [`tracing-honeycomb`](tracing-honeycomb/README.md) contains a concrete implementation that uses [honeycomb.io](https://honeycomb.io) as a backend

## Usage

```toml
[dependencies]
tracing-honeycomb = "0.1"
```

## License

This project is licensed under the terms of the MIT license
