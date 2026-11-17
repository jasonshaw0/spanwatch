# Benchmarks

Measured on 2026-11-16. These are local synthetic microbenchmarks, not production
latency guarantees. Source: [benches/invalidation.rs](../benches/invalidation.rs).
The raw main run is [benchmark-output.txt](benchmark-output.txt), and the
additional partial-coverage run is [benchmark-broad-output.txt](benchmark-broad-output.txt).
[benchmark-results.csv](benchmark-results.csv) contains every point estimate and
95% confidence interval, in nanoseconds, extracted from Criterion's `new` estimates.

## Environment

* Intel Core Ultra 9 275HX, 24 reported cores/logical processors.
* 33,752,997,888 bytes installed physical memory (about 31.4 GiB).
* Windows 11 Pro, version 10.0.26200, x86_64-pc-windows-msvc.
* rustc 1.100.0 (0ef1bc386, 2026-11-09), LLVM 23.1.5; Cargo 1.100.0.
* Criterion 0.8.2, intervaltree 0.2.7; exact dependencies in Cargo.lock.
* Default optimized Cargo bench profile, no custom RUSTFLAGS, LTO, or native-CPU
  optimization requested by this repository.

Runs were on an interactive workstation. CPU affinity, P/E-core selection,
frequency, power mode, thermals, and background services were not controlled.
No other cargo jobs were launched during the recorded measurement runs. Several
confidence intervals are wide; do not infer small performance differences or
portable speedup ratios from these results.

## Method

Thirty samples per case, 100 ms warmup, 500 ms target measurement. Criterion
automatically extended collection for slow cases. There are 45 query cases
(five shapes, three sizes, three implementations) and six setup cases.

Each query benchmark measures an entire batch, including generation advancement,
consumer deduplication, exact witness writes, and result count. It excludes
fixture construction, index compilation, scratch creation, and user processing
of the resulting consumer values. Outputs/buffers are passed through `black_box`.
Owner-set equivalence is asserted outside timing for every case. All three
implementations retain output capacity and generation-stamped owner storage.
The intervaltree baseline adds the same consumer-level deduplication/witness
work to its generic overlap iterator. The linear baseline tests every declared
range for each change; it does not clear a hash set or allocate per batch.

The same deterministic batch is reused in every timed iteration. This measures
warmed repeated queries, and may overstate benefits for cold, varying addresses.
No input/output parsing, application recomputation, or network/disk access is
included. The benchmark compares these particular implementations, not every
possible optimized flat-scan or interval-index implementation.

* Sizes are 256, 4,096, and 65,536 consumer registrations.
* Sparse: four separated length-4 ranges per consumer, in a 64-coordinate slot;
  sixteen deterministic single-coordinate changes select sixteen consumers.
* Dense: the same declarations; one change covers the full dependency extent.
  This exercises the exact full-coverage contiguous-scan fast path.
* Broad: the same declarations; one change covers the middle 90% of the address
  space, leaving both ends out. This cannot use the full-coverage fast path.
* Repeated: the same declarations; 64 identical changes affect one consumer.
* Nested: one nested interval per consumer, with one central change matching all.
* Build: declaration, compilation, and destruction of a fresh four-range index.
* Scratch: allocation, stamp initialization, and destruction for an existing index.

The main run used `cargo bench` (42 cases before adding broad). The supplemental
run used `cargo bench --bench invalidation -- broad` (nine additional cases).
The final `cargo bench` command now runs all 51 cases. Criterion's "change"
lines compare a prior exploratory run; they are not the baseline comparison
reported below. No causal optimization percentage is inferred from those lines.

## Results

Times below are whole-batch point estimates, rounded, not per-dependency latency.
Use CSV confidence intervals and raw output when assessing differences.

| Shape | Consumers | spanwatch | Linear scan | intervaltree + dedup |
| --- | ---: | ---: | ---: | ---: |
| Sparse | 256 | 0.365 us | 7.635 us | 0.794 us |
| Sparse | 4,096 | 0.568 us | 144.451 us | 1.174 us |
| Sparse | 65,536 | 0.974 us | 3,416.985 us | 1.719 us |
| Dense | 256 | 0.666 us | 0.773 us | 5.663 us |
| Dense | 4,096 | 19.357 us | 39.862 us | 206.762 us |
| Dense | 65,536 | 299.503 us | 707.490 us | 6,667.224 us |
| Broad | 256 | 4.124 us | 0.978 us | 6.203 us |
| Broad | 4,096 | 100.883 us | 18.444 us | 117.372 us |
| Broad | 65,536 | 2,527.368 us | 371.234 us | 2,945.022 us |
| Repeated | 256 | 3.260 us | 40.897 us | 2.774 us |
| Repeated | 4,096 | 2.586 us | 684.035 us | 3.923 us |
| Repeated | 65,536 | 3.491 us | 15,507.860 us | 5.468 us |
| Nested | 256 | 1.037 us | 0.506 us | 2.043 us |
| Nested | 4,096 | 16.511 us | 12.447 us | 41.115 us |
| Nested | 65,536 | 359.010 us | 243.899 us | 1,066.478 us |

| Consumers | Build and drop | Fresh scratch and drop |
| ---: | ---: | ---: |
| 256 | 136.989 us | 0.079 us |
| 4,096 | 3.055 ms | 1.247 us |
| 65,536 | 59.608 ms | 25.683 us |

Sparse lookup avoids scanning all dependencies. Dense full-coverage lookup
benefits from contiguous iteration. Nested intervals favor the linear scan;
spanwatch retains tree/ID indirection even though almost nothing can be pruned.
The 256-consumer repeated case is also slower than intervaltree in this run.
Broad partial coverage remains a weakness compared with a flat scan. The index
is useful for sparse changes, not a universal replacement for linear iteration.

## Allocation and complexity

The isolated stats_alloc integration test observes **zero allocations,
reallocations, and deallocations** over 100 full/sparse batches and 100 empty
batches after scratch preparation. Index and scratch construction allocate.
User-provided iterators may allocate independently. A separate regression checks
that retained scratch pointers/capacities remain unchanged over repeated queries.

For n dependencies, w registrations, q changes, and k raw dependency matches:
compile O(n log n); retained memory O(n + w); prepared queries
O(q log(n + 1) + k); result count at most w. Duplicate owners still incur raw
match traversal. Generation wrap performs an O(w) stamp clear. Each worker keeps
its own scratch. Output record storage is the primary scratch cost; the tree's
two ordered ID lists and nodes are the main index overhead.

## Reproduce and next investigations

```text
cargo bench
cargo bench --bench invalidation -- sparse
cargo bench --bench invalidation -- broad
```

Criterion writes data beneath `target/criterion`. CSV values select the `slope`
estimate when available and otherwise `mean`, including its confidence interval.
For conclusions beyond this workstation: pin CPU conditions, increase warmup
and measurement duration, use varied query streams and application datasets,
measure retained/peak memory, and compare against other interval layouts.
Prioritize a measured partial-coverage fallback and lower tree construction
allocation costs before adding dynamic mutation or serialization.
