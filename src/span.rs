use std::{error::Error, fmt, ops::Range};

/// A nonempty half-open range `[start, end)` in a caller-defined coordinate space.
///
/// Endpoints are never incremented, so `u64::MAX` is a valid exclusive end.
/// Adjoining spans such as `0..4` and `4..8` do not overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    start: u64,
    end: u64,
}

impl Span {
    /// Constructs a nonempty span.
    ///
    /// # Errors
    /// Returns [`InvalidSpan`] if `start >= end`.
    pub const fn new(start: u64, end: u64) -> Result<Self, InvalidSpan> {
        if start < end {
            Ok(Self { start, end })
        } else {
            Err(InvalidSpan { start, end })
        }
    }

    /// Returns the inclusive start.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Returns the exclusive end.
    pub const fn end(self) -> u64 {
        self.end
    }

    /// Tests strict overlap, excluding mere adjacency.
    pub const fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Returns the nonempty intersection, or `None` for disjoint/adjacent spans.
    pub fn intersection(self, other: Self) -> Option<Self> {
        Self::new(self.start.max(other.start), self.end.min(other.end)).ok()
    }
}

impl TryFrom<Range<u64>> for Span {
    type Error = InvalidSpan;

    fn try_from(value: Range<u64>) -> Result<Self, Self::Error> {
        Self::new(value.start, value.end)
    }
}

impl From<Span> for Range<u64> {
    fn from(value: Span) -> Self {
        value.start..value.end
    }
}

/// An empty or reversed range rejected by [`Span::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSpan {
    /// The rejected inclusive start.
    pub start: u64,
    /// The rejected exclusive end.
    pub end: u64,
}

impl fmt::Display for InvalidSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "span must satisfy start < end, got {}..{}",
            self.start, self.end
        )
    }
}

impl Error for InvalidSpan {}
