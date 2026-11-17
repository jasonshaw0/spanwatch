# Ecosystem research and concept selection

Research date: 2026-11-16. Network access was available. Sources were searched
with Firecrawl, primary documentation was retrieved, and crates.io was also
queried with `cargo search`. This is a focused survey, not an exhaustive novelty
or trademark search. Search produced no result for `cargo search spanwatch`;
that does not reserve the package name.

## Eight candidates

Scores are engineering judgments from 1 (weak) to 5 (strong), not measurements.
O = differentiation, U = usefulness, T = technical depth, F = feasible complete
implementation, A = adoption opportunity, P = performance opportunity.

| Concept | O | U | T | F | A | P | Existing work and decision |
| --- | -: | -: | -: | -: | -: | -: | --- |
| Batched, explainable range invalidation | 4 | 5 | 4 | 5 | 4 | 5 | intervaltree/coitrees solve interval lookup; Salsa solves broader recomputation. Select the consumer-level batch primitive. |
| Incremental DAG order with cycle witnesses | 3 | 5 | 5 | 3 | 4 | 4 | incremental-topo already maintains an incremental ordering. Cycle diagnostics could differentiate, but deletion and adversarial complexity expand scope. |
| Compressed causal frontiers | 3 | 4 | 5 | 3 | 3 | 5 | Timely already provides antichains; specialized compression needs real workload distributions before choosing a representation. |
| Structured cancellation with cause propagation | 2 | 5 | 4 | 4 | 3 | 3 | tokio-util CancellationToken already has parent/child cancellation and futures. Cause propagation alone is a modest advantage. |
| Safe disjoint-range borrowing and scheduling | 3 | 5 | 5 | 2 | 4 | 4 | disjoint-borrow discussions show an established pain point, but soundness and aliasing deserve a separate, longer audit. |
| Heap-retention regression instrumentation | 3 | 5 | 4 | 3 | 4 | 4 | dhat already profiles and tests heap use. Causal retention is interesting but allocator/platform integration complicates a complete slice. |
| Compact owned/borrowed parser storage | 2 | 4 | 5 | 2 | 3 | 4 | self_cell already supports stable self-referential storage with community review. No compelling general replacement advantage. |
| Compile-time checked binary views | 2 | 5 | 5 | 2 | 3 | 5 | zerocopy provides conversion and layout traits. A new general solution would duplicate substantial audited machinery. |

## Selected concept: spanwatch

Consumers often depend on several separated ranges. Given several writes, the
application needs each affected consumer once, plus a reason it was affected.
Collecting raw interval matches leaves callers to build identity maps, dedup
sets, reusable buffers, and explanation bookkeeping. A full query database may
be disproportionate when the caller already owns scheduling and caching.

The library owns its centered interval tree implementation. It combines:

* one registration for an arbitrary, non-Clone consumer value and many ranges;
* immutable compiled dependencies, allowing shared reads without locks;
* generation-stamped, caller-owned scratch space for batched deduplication;
* one exact change/dependency/intersection witness per affected registration;
* no query-buffer growth when scratch was prepared for the index.

The centered interval tree and generation stamps are established techniques.
The differentiation is the focused API and their integration, not a new theorem.
The hypothesis is that this integration saves repeated application code. The
survey did not establish that no other crate offers the same combination.

## Verified comparisons

* [intervaltree](https://docs.rs/intervaltree/0.2.7/intervaltree/) is an immutable
  generic interval index with overlap iterators. It is the closest general
  alternative, and is included as a benchmark with consumer deduplication added.
* [coitrees](https://github.com/dcjones/coitrees) targets static integer overlap
  queries, cache locality, genomic workloads, SIMD, and reuse between sorted
  queries. spanwatch does not claim a faster raw interval tree.
* [rangemap](https://docs.rs/rangemap/latest/rangemap/) maps ranges to values and
  coalesces contiguous/overlapping ranges with equal values. That is different
  from retaining arbitrary overlapping consumers and individual witnesses.
* [Salsa overview](https://salsa-rs.github.io/salsa/overview.html) describes
  tracked deterministic computations, memoization, and a database. spanwatch
  supplies dependency selection only; it does not recompute or cache values.

Other primary sources evaluated:

* [incremental-topo on crates.io](https://crates.io/crates/incremental-topo)
* [Timely Antichain](https://docs.rs/timely/latest/timely/progress/frontier/struct.Antichain.html)
* [CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)
* [self_cell](https://docs.rs/self_cell/latest/self_cell/)
* [dhat](https://docs.rs/dhat/latest/dhat/)
* [zerocopy](https://docs.rs/zerocopy/latest/zerocopy/)
* [nodit](https://docs.rs/nodit/latest/nodit/) for non-overlapping discrete maps

Relevant technical discussion:
[disjoint range borrowing release thread](https://www.reddit.com/r/rust/comments/cyf9v5/disjointborrow_released_mutably_borrow_disjoint/).
This is a pain-point signal, not evidence of market size or an absence of solutions.

Documentation for benchmark and property-test integration was obtained with
Context7 from the Criterion and Proptest upstream projects:
[Criterion](https://github.com/criterion-rs/criterion.rs) and
[Proptest](https://github.com/proptest-rs/proptest).

## Scope decision

Use fixed `u64` half-open, nonempty coordinates. A coordinate space belongs to
the caller: use one index per file/buffer or encode a stable global domain.
Range shifting after text insertion, dynamic dependency replacement, transitive
invalidation, async execution, persistence, and multi-dimensional ranges are
deliberately outside v0.1. This keeps the first slice independently testable.
