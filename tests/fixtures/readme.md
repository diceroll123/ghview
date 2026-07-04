# widget-tui

[![Crate Badge](https://img.shields.io/badge/crates.io-widget--tui-blue)](https://example.com/crates/widget-tui)
[![Docs Badge](https://img.shields.io/badge/docs-widget--tui-blue)](https://example.com/docs/widget-tui)
[![CI Badge](https://img.shields.io/badge/CI-passing-brightgreen)](https://example.com/ci/widget-tui)
[![License Badge](https://img.shields.io/badge/license-MIT-lightgrey)](./LICENSE)

widget-tui is a small library for building terminal user interfaces. It focuses on a simple
widget model, predictable layout rules, and minimal dependencies.

## Table of Contents

- [Quickstart](#quickstart)
- [Documentation](#documentation)
- [Examples](#examples)
- [Contributing](#contributing)
- [License](#license)

## Quickstart

Add the crate to your project:

```shell
cargo add widget-tui
```

A minimal application looks like this:

```rust
use widget_tui::{Terminal, Frame};

fn main() -> anyhow::Result<()> {
    let mut terminal = Terminal::new()?;
    loop {
        terminal.draw(render)?;
        if widget_tui::poll_quit()? {
            break;
        }
    }
    Ok(())
}

fn render(frame: &mut Frame) {
    frame.render_text("hello, terminal");
}
```

## Documentation

- [Guide](https://example.com/docs/widget-tui/guide) - concepts and walkthroughs.
- [API Reference](https://example.com/docs/widget-tui/api) - generated API documentation.
- [Changelog](./CHANGELOG.md) - notable changes between releases.

## Examples

The `examples/` directory contains small, focused programs:

- `examples/list.rs` - a scrollable list widget.
- `examples/tabs.rs` - switching between multiple views.
- `examples/form.rs` - basic keyboard-driven input handling.

## Contributing

Contributions are welcome. Please open an issue before starting on a large change, and make sure
`cargo test` and `cargo clippy` pass before opening a pull request. See [CONTRIBUTING.md] for
details.

[CONTRIBUTING.md]: ./CONTRIBUTING.md

## License

This project is licensed under the [MIT License](./LICENSE).
