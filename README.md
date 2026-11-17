# spanwatch

[![CI](https://github.com/jasonshaw0/spanwatch/actions/workflows/ci.yml/badge.svg)](https://github.com/jasonshaw0/spanwatch/actions/workflows/ci.yml)

Batched range invalidation with deduplicated consumers and exact explanations.

Declare which regions each consumer reads. Compile once, then submit batches of
writes. Each affected registration appears once, even when several dependencies
or writes overlap, accompanied by an original dependency and its intersection
with the earliest matching change.

```rust
use spanwatch::{Builder, Span};

fn main() -> Result<(), spanwatch::InvalidSpan> {
    let mut builder = Builder::new();
    builder.watch("header", [Span::new(0, 64)?]);
    builder.watch("checksum", [Span::new(0, 64)?, Span::new(512, 1024)?]);
    let index = builder.build();
    let mut scratch = index.scratch();

    for hit in index.query([Span::new(32, 40)?, Span::new(700, 710)?], &mut scratch) {
        println!("{} affected at {:?}", hit.value, hit.cause.overlap);
    }
    Ok(())
}
```

Both consumers are selected; `checksum` appears once. Consumer values need no
`Clone`, `Eq`, or `Hash` implementation. Each `watch` call is a separate identity,
even if values are equal. Values are borrowed in results.

Rust 2024; MSRV 1.88. Zero runtime dependencies. Unsafe code is forbidden.
The source is public on [GitHub](https://github.com/jasonshaw0/spanwatch).
The crate has not been published to crates.io. Use a Git dependency to try the
current development version:

```toml
[dependencies]
spanwatch = { git = "https://github.com/jasonshaw0/spanwatch" }
```

The centered interval tree supports O(log(n + 1) + k) range lookup; batching adds
generation-stamped consumer deduplication. Prepared scratch avoids internal heap
allocation during queries. Dense and highly overlapping workloads may favor a
flat scan. See [architecture](docs/architecture.md), [research and alternatives](docs/research.md),
and [benchmark methodology and measurements](docs/benchmarks.md).
The [verification record](docs/verification.md) lists executed checks and limits.

Ranges are nonempty, half-open `u64` coordinates: adjacent spans do not overlap.
Use one index per coordinate space. Coordinates do not shift automatically;
rebuild after dependencies change. Empty dependency lists create inert consumers.
Results follow input change order, then lookup traversal, not registration order.
The witness identifies the earliest matching change and one dependency; it does
not enumerate every cause. `registration` is a local `watch` ordinal, not a
cross-index ID. Scratch can be reused with different indices and grows as needed.

Share the immutable index across threads with one scratch per worker. No async
runtime, scheduler, memoized values, or transitive dependency graph is included.

Run `cargo run --example binary_cache` or `cargo run --example shared_index`.
Contributors: see [CONTRIBUTING.md](CONTRIBUTING.md). License: MIT OR Apache-2.0.
