//! Select affected consumers from a batch of changed ranges, once per registration.
//!
//! `spanwatch` compiles many-to-many range dependencies into an immutable interval
//! index. Each result borrows its consumer value and carries an exact witness.
//! It has no runtime dependencies, hidden synchronization, or unsafe code.
//!
//! ```
//! use spanwatch::{Builder, Span};
//!
//! let mut builder = Builder::new();
//! builder.watch("header", [Span::new(0, 64)?]);
//! builder.watch("checksum", [Span::new(0, 64)?, Span::new(512, 1024)?]);
//! let index = builder.build();
//! let mut scratch = index.scratch();
//! let hits: Vec<_> = index.query(
//!     [Span::new(32, 40)?, Span::new(700, 710)?], &mut scratch,
//! ).collect();
//! assert_eq!(hits.len(), 2); // checksum appears once, despite both changes
//! assert!(hits.iter().all(|hit| hit.cause.change_index == 0));
//! # Ok::<(), spanwatch::InvalidSpan>(())
//! ```
//!
//! Ranges are nonempty, half-open `u64` coordinates. Use one index per coordinate
//! space. Offsets are fixed: text insertions and deletions require caller-side
//! rebasing and rebuilding. This crate selects consumers; it does not execute
//! them or propagate invalidation through a graph.

mod span;
mod tree;

use std::{iter::FusedIterator, slice};

pub use span::{InvalidSpan, Span};
use tree::{Dependency, Tree};

/// Declares consumers and their read ranges before compiling an [`Index`].
///
/// Every [`watch`](Self::watch) call creates a distinct registration; equal
/// consumer values are not merged. Consumer values need no trait bounds.
#[derive(Debug)]
pub struct Builder<T> {
    values: Vec<T>,
    dependencies: Vec<Dependency>,
}

impl<T> Builder<T> {
    /// Creates an empty builder without allocating.
    pub const fn new() -> Self {
        Self {
            values: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    /// Adds one consumer with any number of dependencies, moving the value once.
    ///
    /// An empty iterator creates an inert consumer, which will never be reported.
    /// Duplicate or overlapping dependencies are allowed. If the supplied
    /// iterator panics, the consumer and dependencies consumed so far remain
    /// registered; the builder remains valid if the panic is caught.
    pub fn watch(&mut self, value: T, spans: impl IntoIterator<Item = Span>) -> &mut Self {
        let owner = self.values.len();
        self.values.push(value);
        self.dependencies
            .extend(spans.into_iter().map(|span| Dependency { span, owner }));
        self
    }

    /// Consumes the declarations and compiles an immutable interval index.
    ///
    /// Uses O(n log n) time and O(n + w) retained memory for n dependencies and
    /// w registrations. No consumer values are cloned.
    pub fn build(self) -> Index<T> {
        Index {
            values: self.values,
            tree: Tree::build(self.dependencies),
        }
    }
}

impl<T> Default for Builder<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable, shareable declarations supporting batched range invalidation.
///
/// `Index<T>` is `Send` and `Sync` whenever `T` is. Concurrent readers use one
/// [`Scratch`] per worker. Changing dependencies requires building a new index.
#[derive(Debug)]
pub struct Index<T> {
    values: Vec<T>,
    tree: Tree,
}

impl<T> Index<T> {
    /// Returns the number of consumer registrations, including inert ones.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether there are no registered consumers.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the number of declared ranges, including duplicates.
    pub fn dependency_count(&self) -> usize {
        self.tree.len()
    }

    /// Allocates scratch with enough space for any batch on this index.
    ///
    /// Subsequent queries on this index do not allocate internally. The supplied
    /// change iterator itself may allocate. Scratch may also be reused with
    /// other indices; it grows automatically when necessary.
    pub fn scratch(&self) -> Scratch {
        let mut scratch = Scratch::new();
        scratch.prepare(self.len());
        scratch
    }

    /// Eagerly selects each affected registration once, returning borrowed results.
    ///
    /// The witness uses the earliest overlapping change in input order and one
    /// matching original dependency. Results follow change order and deterministic
    /// lookup traversal, **not** registration order. An empty batch yields no hits.
    /// An unconsumed result iterator still represents a fully evaluated batch.
    ///
    /// With prepared scratch: O(q log(n + 1) + k) time for q changes, n declared
    /// ranges and k raw overlaps (including duplicates). Scratch growth costs
    /// O(w); generation wrap clears stamps once per 2^64 batches.
    ///
    /// Scratch cannot be overwritten while its results are still in use:
    /// ```compile_fail
    /// use spanwatch::{Builder, Span};
    /// let index = Builder::<()>::new().build();
    /// let mut scratch = index.scratch();
    /// let mut hits = index.query([], &mut scratch);
    /// let _other = index.query([], &mut scratch);
    /// hits.next();
    /// ```
    pub fn query<'index, 'scratch>(
        &'index self,
        changes: impl IntoIterator<Item = Span>,
        scratch: &'scratch mut Scratch,
    ) -> Matches<'index, 'scratch, T> {
        scratch.begin(self.len());
        for (change_index, change) in changes.into_iter().enumerate() {
            self.tree.visit(change, &mut |dependency| {
                if scratch.stamps[dependency.owner] != scratch.generation {
                    scratch.stamps[dependency.owner] = scratch.generation;
                    scratch.hits.push(Record {
                        owner: dependency.owner,
                        cause: Cause {
                            change_index,
                            change,
                            dependency: dependency.span,
                            overlap: change
                                .intersection(dependency.span)
                                .expect("tree reports only overlapping dependencies"),
                        },
                    });
                }
            });
        }
        Matches {
            values: &self.values,
            records: scratch.hits.iter(),
        }
    }
}

/// Reusable per-worker query memory, independent of any index's lifetime.
///
/// Uses O(w) space for the largest registration count seen. Memory is retained
/// between queries; drop the scratch to release it. No consumer values are held.
#[derive(Debug, Default)]
pub struct Scratch {
    stamps: Vec<u64>,
    generation: u64,
    hits: Vec<Record>,
}

impl Scratch {
    /// Creates empty scratch without allocating; its first query may allocate.
    pub const fn new() -> Self {
        Self {
            stamps: Vec::new(),
            generation: 0,
            hits: Vec::new(),
        }
    }

    fn prepare(&mut self, owners: usize) {
        if self.stamps.len() < owners {
            self.stamps.resize(owners, 0);
        }
        self.hits.clear();
        if self.hits.capacity() < owners {
            self.hits.reserve(owners);
        }
    }

    fn begin(&mut self, owners: usize) {
        self.prepare(owners);
        self.generation = match self.generation.checked_add(1) {
            Some(next) => next,
            None => {
                self.stamps.fill(0);
                1
            }
        };
    }
}

/// One exact explanation for an affected consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cause {
    /// Zero-based ordinal of the earliest input change affecting this consumer.
    pub change_index: usize,
    /// The original changed span, without coalescing.
    pub change: Span,
    /// One original declared dependency overlapping that change.
    pub dependency: Span,
    /// The exact nonempty intersection of `change` and `dependency`.
    pub overlap: Span,
}

#[derive(Debug)]
struct Record {
    owner: usize,
    cause: Cause,
}

/// A borrowed consumer and the reason it appears in the current batch.
#[derive(Debug)]
pub struct Hit<'index, T> {
    /// The consumer value, borrowed directly from the index.
    pub value: &'index T,
    /// Zero-based `watch` call ordinal within this index; not a global identity.
    pub registration: usize,
    /// One exact overlap witness, using the earliest matching input change.
    pub cause: Cause,
}

/// Exact-size, double-ended iterator over an eagerly evaluated query.
///
/// Hits borrow their consumer values from the index, so they may outlive the
/// scratch borrow but cannot outlive the index:
/// ```compile_fail
/// use spanwatch::{Builder, Scratch, Span};
/// let mut scratch = Scratch::new();
/// let value;
/// {
///     let mut builder = Builder::new();
///     builder.watch(String::from("owned"), [Span::new(0, 10).unwrap()]);
///     let index = builder.build();
///     value = index.query([Span::new(1, 2).unwrap()], &mut scratch).next().unwrap().value;
/// }
/// println!("{value}");
/// ```
#[derive(Debug)]
pub struct Matches<'index, 'scratch, T> {
    values: &'index [T],
    records: slice::Iter<'scratch, Record>,
}

impl<'index, T> Iterator for Matches<'index, '_, T> {
    type Item = Hit<'index, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(|record| Hit {
            value: &self.values[record.owner],
            registration: record.owner,
            cause: record.cause,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.records.size_hint()
    }
}

impl<T> DoubleEndedIterator for Matches<'_, '_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.records.next_back().map(|record| Hit {
            value: &self.values[record.owner],
            registration: record.owner,
            cause: record.cause,
        })
    }
}

impl<T> ExactSizeIterator for Matches<'_, '_, T> {}
impl<T> FusedIterator for Matches<'_, '_, T> {}

// Compile and execute the README's public quickstart as part of `cargo test`.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct Readme;

#[cfg(test)]
mod tests {
    use super::*;

    fn index(owners: usize) -> Index<usize> {
        let mut builder = Builder::new();
        for owner in 0..owners {
            builder.watch(owner, [Span::new(0, 100).unwrap()]);
        }
        builder.build()
    }

    #[test]
    fn generation_wrap_clears_all_retained_stamps() {
        let large = index(100);
        let small = index(1);
        let mut scratch = large.scratch();
        let change = Span::new(0, 1).unwrap();
        assert_eq!(large.query([change], &mut scratch).len(), 100);
        scratch.generation = u64::MAX;
        // Wrap on the small index must clear the tail, too: the next generation
        // on the large index must not mistake old stamps for this batch.
        scratch.stamps.fill(2);
        assert_eq!(small.query([change], &mut scratch).len(), 1);
        assert_eq!(scratch.generation, 1);
        assert_eq!(large.query([change], &mut scratch).len(), 100);
    }

    #[test]
    fn prepared_storage_does_not_grow_or_move_across_queries() {
        let index = index(1024);
        let mut scratch = index.scratch();
        let stamps = (scratch.stamps.as_ptr(), scratch.stamps.capacity());
        let hits = (scratch.hits.as_ptr(), scratch.hits.capacity());
        for _ in 0..100 {
            assert_eq!(
                index
                    .query([Span::new(0, 1).unwrap(); 4], &mut scratch)
                    .len(),
                1024
            );
            assert_eq!((scratch.stamps.as_ptr(), scratch.stamps.capacity()), stamps);
            assert_eq!((scratch.hits.as_ptr(), scratch.hits.capacity()), hits);
        }
    }
}
