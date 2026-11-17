# Architecture

## Problem and use cases

Map a batch of modified coordinate ranges to the consumers whose declared read
ranges overlap. Emit each registration once, with an exact witness. Applications
include fixed-layout binary parsers, block-cache invalidation, mapped-buffer
views, and incremental analysis with stable offsets. The application owns
recomputation, revisions, and coordinate-space identity.

See [research](research.md) for comparison with intervaltree, coitrees, rangemap,
and Salsa. The index is implemented here rather than wrapping another crate.

## Proposed public contract

```rust
use spanwatch::{Builder, Span};

let mut builder = Builder::new();
builder.watch("header", [Span::new(0, 64)?]);
builder.watch("checksum", [Span::new(0, 64)?, Span::new(512, 1024)?]);
let index = builder.build();
let mut scratch = index.scratch();
for hit in index.query([Span::new(32, 40)?, Span::new(700, 710)?], &mut scratch) {
    println!("{}: {:?}", hit.value, hit.cause.overlap);
}
# Ok::<(), spanwatch::InvalidSpan>(())
```

Each `watch` call creates a distinct registration, even if values compare equal.
Values need no trait bounds. Empty dependency lists are legal inert consumers.
Duplicate and overlapping dependencies are preserved. Results follow change
input order, then deterministic lookup traversal; consumer registration order is
not promised. The witness uses the earliest input change that overlaps the
consumer, and one matching original dependency. It is not a list of all causes.

## Data structures and algorithms

1. `Builder<T>` owns values and `(Span, owner_index)` dependencies.
2. Compilation sorts dependency IDs by start, choosing the median start as each
   node's center. Partition into intervals ending at/before center, intervals
   containing center, and intervals starting after center. The median interval
   itself contains center, ensuring progress. Both children have at most half
   the parent's intervals (up to rounding).
3. Nodes live in one vector. Each node holds child indices and a range into two
   flat ID vectors: crossing intervals sorted by increasing start and by
   decreasing end. Original dependencies are stored once for witness recovery.
   Start order is preserved while partitioning; only crossing end lists are
   separately sorted.
4. For `[a,b)` with `b <= center`, scan starts until `start >= b`, then recurse
   left. For `a > center`, scan descending ends until `end <= a`, then recurse
   right. Otherwise every crossing interval overlaps; scan all and search both
   relevant children. Strict comparisons enforce half-open semantics.
5. If a change covers the full bounding extent of all dependencies, scan original
   records contiguously. There is nothing for the tree to prune. This exact
   coverage fast path addresses a measured dense-query bottleneck; partial
   coverage still uses the tree. Traversal order is not part of the API contract.
6. A scratch vector has a generation stamp per owner. On first match in a batch,
   stamp the owner and append `(owner, cause)`. Later matches do not append.
   Advancing the generation normally replaces an O(owner_count) clear. On
   `u64` generation wrap, clear all stamps and restart at one.

## Boundaries and dependencies

`span.rs` owns validated coordinates and errors; `tree.rs` is a private interval
index; `lib.rs` owns registrations, batch deduplication, witnesses and iterators.
The runtime has zero external dependencies and forbids unsafe Rust. Dev-only
Criterion, Proptest, intervaltree, and stats_alloc provide measurement, generated
tests, an existing comparison index, and isolated allocation instrumentation.

## Ownership and lifetime design

Compilation consumes the builder and moves values without cloning. The index
is immutable. Query scratch owns stamps and compact result records, not consumer
values. `Matches<'index, 'scratch, T>` borrows the index and a slice of scratch;
each iterator item contains `&'index T` and a copied witness. Consumer references
may outlive the iterator and scratch borrow, but not the index. Another query
requires exclusive access to the same scratch, enforced by borrowing.

Scratch is unbranded and may be reused with other indices. It grows as needed,
never shrinks automatically, and starts a new generation for every query, so
old owner positions cannot suppress results in a different index.

## Errors and panics

`Span::new` and `TryFrom<Range<u64>>` reject `start >= end` with an `InvalidSpan`
carrying both bounds. Empty ranges are not silently treated as edits. Spans do
not add endpoints, avoiding overflow at `u64::MAX`. The maximum endpoint is
exclusive, so the coordinate `u64::MAX` itself cannot be covered.

Queries accept already validated spans, so normal queries are infallible.
Allocation failure follows standard Vec behavior. User-supplied iterators may
panic; registration first stores its value, preserving index consistency if a
dependency iterator panics. A later query clears partial prior output and uses
a new generation. Infinite iterators are the caller's responsibility.

## Concurrency

`Index<T>` is Send/Sync whenever T is. Workers share `&Index<T>` or `Arc<Index<T>>`
and keep one scratch per worker. No hidden locks, atomics, runtime, callbacks,
or global IDs are needed. This is a synchronous selection primitive.

## Complexity and allocation expectations

Let n be dependency count, w owner count, q change count, and k the total raw
dependency matches across changes (including repeated owners).

* Compile: O(n log n) time, O(n + w) retained memory; temporary partition
  vectors and O(log n) recursive depth.
* Each interval query: O(log(n + 1) + matches); a branch into both children
  reports at least one crossing interval at that node, charging those visits
  to output. Batch: O(q log(n + 1) + k), excluding growth and rare stamp wrap.
* Output: at most w witnesses; duplicate owners still cost raw match traversal.
* Scratch construction/growth: O(w); prepared queries use no heap allocation
  internally. Recursion uses O(log(n + 1)) stack; result iteration is O(1)/hit.
* Stamp wrap: O(maximum owner count seen by that scratch), once per 2^64 batches.

Broad changes or heavily overlapping dependencies can make a flat scan faster.
The two ID orderings and node records trade memory for output-sensitive lookup.
No stronger general performance claim is made without measurements.

## Validation plan

Start with span boundaries and a complete two-consumer batch, then exercise
empty indices, nesting, duplicates, adjacency, maximum endpoints, scratch reuse,
generation wrap, iterator panics, non-Clone values, thread sharing and lifetime
misuse. Property tests compare owner sets with a simple linear oracle, verify
witness validity and earliest-change provenance, and compare intervaltree.
Exhaustive small-domain and adversarial shapes cover branch boundaries.
Criterion measures sparse, dense, repeated-change and nested workloads against
a prepared linear scan and intervaltree with equivalent consumer deduplication.
Build and fresh-scratch costs are measured separately. See benchmarks.md for
the actual environment and results once measured.

## Tradeoffs and follow-ups

No dynamic mutation, coordinate rebasing, empty insertion events, full provenance
enumeration, equality-based deduplication, transitive dependency graph, or
serialized index format. Rebuild after changing declarations. Benchmark real
consumer distributions and memory footprint before selecting a compressed
layout, specialized dense-query path, or mutable overlay.
