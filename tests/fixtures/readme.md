# noop-svc

[![Crate Badge](https://img.shields.io/badge/crates.io-noop--svc-blue)](https://example.com/crates/noop-svc)
[![Docs Badge](https://img.shields.io/badge/docs-noop--svc-blue)](https://example.com/docs/noop-svc)
[![CI Badge](https://img.shields.io/badge/CI-passing-brightgreen)](https://example.com/ci/noop-svc)
[![License Badge](https://img.shields.io/badge/license-MIT-lightgrey)](./LICENSE)

noop-svc is NoOps, as a Service. Point any client at it and every request gets a fast,
predictable `200 OK` and an empty body. No handlers to write, no state to manage, nothing to
break. It's the backend you get when you don't need a backend yet.

## Table of Contents

- [Quickstart](#quickstart)
- [Documentation](#documentation)
- [Examples](#examples)
- [Contributing](#contributing)
- [License](#license)

## Quickstart

Add the crate to your project:

```shell
cargo add noop-svc
```

A minimal server looks like this:

```rust
use noop_svc::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Server::bind("0.0.0.0:8080")
        .respond_with_nothing()
        .serve()
        .await
}
```

Every route, every method, every body: `200 OK`, zero bytes, done.

## Documentation

- [Guide](https://example.com/docs/noop-svc/guide) - concepts and configuration options.
- [API Reference](https://example.com/docs/noop-svc/api) - generated API documentation.
- [Changelog](./CHANGELOG.md) - notable changes between releases.

## Examples

The `examples/` directory contains small, focused programs:

- `examples/health.rs` - a health-check endpoint that never fails.
- `examples/latency.rs` - injecting an artificial (configurable) response delay.
- `examples/logging.rs` - logging requests before doing nothing with them.

## Contributing

Contributions are welcome. Please open an issue before starting on a large change, and make sure
`cargo test` and `cargo clippy` pass before opening a pull request. See [CONTRIBUTING.md] for
details.

[CONTRIBUTING.md]: ./CONTRIBUTING.md

## License

This project is licensed under the [MIT License](./LICENSE).
