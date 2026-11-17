# Local verification

Executed on Windows x86_64 on 2026-11-16. Library code is Rust 2024. The declared
minimum toolchain is Rust 1.88; the current stable toolchain here is Rust 1.100.

* `cargo fmt --all -- --check`: passed.
* `cargo clippy --all-targets --all-features -- -D warnings`: passed.
* `cargo test --all-features`: passed with `PROPTEST_CASES=10000`.
* `cargo +1.88.0 test --locked --all-features`: passed with default case counts.
* `cargo doc --no-deps` with `RUSTDOCFLAGS=-D warnings`: passed.
* `cargo run --example binary_cache`: selected header, checksum, and payload,
  with checksum selected once and witnesses from the appropriate input changes.
* `cargo run --example shared_index`: four threads each selected their page
  using independent scratch and a shared index.
* `cargo bench`: completed 42 original cases; subsequent broad-only measurement
  completed nine supplemental cases. See [benchmarks](benchmarks.md) and raw logs.
* `cargo bench --bench invalidation -- --test`: all 51 final benchmark cases
  passed their execution and baseline-equivalence smoke checks.
* `cargo package --locked --allow-dirty`: built and verified the distributable
  crate. Cargo's metadata warning reflects the intentionally unset public
  documentation/homepage/repository URLs; those must use real maintainer-owned
  destinations before release.

The test suite has 19 normal tests and four documentation tests, including two
compile-fail borrow/lifetime checks and the executable README quickstart.
The two property tests exercised 20,000 generated inputs in the extended stable
run and 2,048 per default run. Exhaustive small-domain testing checks 1,296
two-change batches. Tests cover invalid/maximum endpoints, adjacency, duplicate
dependencies/values, inert consumers, nesting, first-change provenance, full
coverage, panic recovery, scratch reuse, generation wrap, and thread sharing.

The allocation test runs in a separate process with stats_alloc and observed
zero allocation/reallocation/deallocation calls during prepared query loops.
The crate itself contains no unsafe blocks; unsafe code is forbidden by Cargo
lints. Dependencies used for testing may use unsafe internally.

GitHub Actions configuration covers stable Windows/Linux and MSRV Linux.
Hosted CI and non-Windows execution have not been observed locally. There is no
hosted repository URL yet, and no publishing action runs automatically.

## Suggested development milestones

1. Integrate with a real fixed-layout parser or block cache; collect varying
   query distributions, retained memory, and rebuild frequency.
2. Evaluate an adaptive scan for broad partial changes and reduce build-time
   partition allocation if profiles show that these costs matter.
3. Consider dynamic dependency overlays only with a concrete workload and
   explicit revision/identity semantics. Coordinate rebasing is a separate API.
4. Before release, set real repository metadata, confirm crate-name ownership,
   obtain maintainer/API review, run hosted cross-platform CI, and publish with
   explicit release authorization.
