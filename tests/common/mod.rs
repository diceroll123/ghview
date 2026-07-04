#![allow(dead_code)]

pub mod builders;
pub mod gh_mock;
pub mod inflate;

pub const FIXED_NOW: &str = "2026-01-15T12:00:00Z";

pub fn fixed_now() -> jiff::Timestamp {
    FIXED_NOW.parse().expect("valid FIXED_NOW timestamp")
}

pub fn fixture(name: &str) -> String {
    let path =
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/")).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
}
