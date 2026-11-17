# Contributing

Use stable Rust 1.88 or newer. The runtime crate has no dependencies; dependencies
used for testing and benchmarking are development-only. No feature flags exist
because the current API has no optional subsystem.

Read [architecture](docs/architecture.md) and [research](docs/research.md) before
proposing API changes. Preserve half-open boundaries, registration identity,
earliest-change witnesses, and caller-owned scratch. Include a realistic use
case with an API proposal. New runtime dependencies need a concrete justification.

Run from the repository root:

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps
cargo run --example binary_cache
cargo run --example shared_index
cargo bench
cargo package --locked
```

For a larger randomized run, set `PROPTEST_CASES=10000` in your shell and run
`cargo test --test properties`. Keep shrunk failure seeds in version control.
The allocation integration test runs alone in its own process and checks only
prepared queries, excluding index/scratch creation and user iterator allocations.
See [benchmark notes](docs/benchmarks.md) for workloads and measurement caveats.

Changes must remain safe Rust. The manifest forbids unsafe code in this package.
Add focused regressions for semantic changes and use a linear oracle for interval
logic. Avoid tests which merely repeat implementation details. Performance changes
need equivalent baseline work, recorded environment, and actual measurements.

CI checks stable on Windows and Linux and the declared minimum compiler on Linux.
See [GitHub Actions](https://github.com/jasonshaw0/spanwatch/actions/workflows/ci.yml)
for hosted run results.

Before publishing to crates.io: recheck name availability, review the packaged
source and licensing, rerun checks,
and confirm release authorization. This repository does not publish automatically.

Unless you explicitly state otherwise, intentionally submitted contributions are
licensed under both MIT and Apache-2.0, matching this project.
