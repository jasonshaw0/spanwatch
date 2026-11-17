//! Regression for the full-extent scan fast path and adjacent queries.
use spanwatch::{Builder, Span};

#[test]
fn full_coverage_and_partial_queries_have_exact_earliest_witnesses() {
    let s = |a, b| Span::new(a, b).unwrap();
    let mut builder = Builder::new();
    builder.watch("two regions", [s(10, 20), s(80, 90)]);
    builder.watch("nested", [s(30, 70)]);
    builder.watch("inert", []);
    let index = builder.build();
    let mut scratch = index.scratch();
    for full in [s(10, 90), s(0, u64::MAX)] {
        let hits: Vec<_> = index.query([s(85, 86), full], &mut scratch).collect();
        assert_eq!(hits.len(), 2);
        assert_eq!(*hits[0].value, "two regions");
        assert_eq!(hits[0].cause.change_index, 0);
        assert_eq!(hits[0].cause.overlap, s(85, 86));
        assert_eq!(*hits[1].value, "nested");
        assert_eq!(hits[1].cause.change_index, 1);
        assert_eq!(hits[1].cause.overlap, s(30, 70));
    }
    assert_eq!(
        index.query([s(0, 10), s(90, u64::MAX)], &mut scratch).len(),
        0
    );
    assert_eq!(index.query([s(11, 89)], &mut scratch).len(), 2);
}
