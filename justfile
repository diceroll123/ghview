fmt:
    cargo fmt

lint:
    cargo fmt --check
    cargo clippy -- -D warnings

fix:
    cargo fmt
    cargo clippy --fix --allow-dirty
