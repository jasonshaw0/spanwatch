//! Public API behavior, boundaries, and ownership.
use spanwatch::{Builder, InvalidSpan, Scratch, Span};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn span(start: u64, end: u64) -> Span {
    Span::new(start, end).unwrap()
}

#[test]
fn complete_batch_deduplicates_and_explains() {
    let mut builder = Builder::new();
    builder.watch("header", [span(0, 64)]);
    builder.watch("checksum", [span(0, 64), span(512, 1024)]);
    let index = builder.build();
    let mut scratch = index.scratch();
    let hits: Vec<_> = index
        .query([span(32, 40), span(700, 710)], &mut scratch)
        .collect();
    assert_eq!(hits.len(), 2);
    for hit in hits {
        assert_eq!(hit.cause.change_index, 0);
        assert_eq!(hit.cause.overlap, span(32, 40));
    }
}

#[test]
fn range_validation_and_extremes() {
    for (a, b) in [(0, 0), (9, 4), (u64::MAX, u64::MAX)] {
        let error = Span::new(a, b).unwrap_err();
        assert_eq!(error, InvalidSpan { start: a, end: b });
        assert!(error.to_string().contains("start < end"));
        let _: &dyn std::error::Error = &error;
    }
    let full = span(0, u64::MAX);
    assert_eq!(full.start(), 0);
    assert_eq!(full.end(), u64::MAX);
    assert_eq!(Span::try_from(0..u64::MAX).unwrap(), full);
    let range: std::ops::Range<u64> = full.into();
    assert_eq!(range, 0..u64::MAX);
    assert_eq!(
        full.intersection(span(u64::MAX - 1, u64::MAX)),
        Some(span(u64::MAX - 1, u64::MAX))
    );
    assert!(!span(0, 1).overlaps(span(1, 2)));
    assert_eq!(span(0, 1).intersection(span(1, 2)), None);
}

#[test]
fn empty_inputs_and_inert_registrations() {
    let index = Builder::<()>::default().build();
    assert!(index.is_empty());
    assert_eq!(index.len(), 0);
    assert_eq!(index.dependency_count(), 0);
    let mut scratch = Scratch::default();
    assert_eq!(index.query([span(0, 10)], &mut scratch).len(), 0);
    let mut builder = Builder::new();
    builder.watch((), []);
    let index = builder.build();
    assert!(!index.is_empty());
    assert_eq!(index.len(), 1);
    assert_eq!(index.dependency_count(), 0);
    assert_eq!(index.query([span(0, 10)], &mut scratch).len(), 0);
    assert_eq!(index.query([], &mut scratch).len(), 0);
}

#[test]
fn identities_duplicates_adjacency_and_original_dependencies() {
    let mut builder = Builder::new();
    builder.watch("same", [span(0, 5), span(0, 5), span(2, 8)]);
    builder.watch("same", [span(5, 10)]);
    builder.watch("adjacent", [span(10, 20)]);
    let index = builder.build();
    assert_eq!(index.dependency_count(), 5);
    let mut scratch = index.scratch();
    let hits: Vec<_> = index
        .query([span(8, 10), span(0, 1), span(0, 1)], &mut scratch)
        .collect();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].registration, 1);
    assert_eq!(hits[0].cause.change_index, 0);
    assert_eq!(hits[0].cause.overlap, span(8, 10));
    assert_eq!(hits[1].registration, 0);
    assert_eq!(hits[1].cause.change_index, 1);
    assert_eq!(hits[1].cause.dependency, span(0, 5));
}

#[test]
fn scratch_reuse_across_different_sized_indices_and_empty_batches() {
    let mut scratch = Scratch::new();
    for size in [1, 200, 0, 17, 300, 1] {
        let mut builder = Builder::new();
        for owner in 0..size {
            builder.watch(owner, [span(0, 1)]);
        }
        let index = builder.build();
        for _ in 0..3 {
            assert_eq!(index.query([span(0, 1)], &mut scratch).len(), size);
            assert_eq!(index.query([], &mut scratch).len(), 0);
            assert_eq!(index.query([span(1, 2)], &mut scratch).len(), 0);
        }
    }
}

#[test]
fn non_clone_values_and_borrowed_hits_outlive_scratch() {
    #[derive(Debug)]
    struct Consumer(String);
    let mut builder = Builder::new();
    builder.watch(Consumer("parser".into()), [span(0, 1)]);
    let index = builder.build();
    let value = {
        let mut scratch = index.scratch();
        index
            .query([span(0, 1)], &mut scratch)
            .next()
            .unwrap()
            .value
    };
    assert_eq!(value.0, "parser");
}

#[test]
fn iterator_is_exact_double_ended_fused_and_eager() {
    let mut builder = Builder::new();
    for i in 0..7 {
        builder.watch(i, [span(0, 1)]);
    }
    let index = builder.build();
    let mut scratch = index.scratch();
    let mut consumed = 0;
    let changes = [span(0, 1), span(0, 2)]
        .into_iter()
        .inspect(|_| consumed += 1);
    let mut results = index.query(changes, &mut scratch);
    assert_eq!(consumed, 2);
    assert_eq!(results.size_hint(), (7, Some(7)));
    assert_eq!(results.next().unwrap().registration, 0);
    assert_eq!(results.next_back().unwrap().registration, 6);
    assert_eq!(results.len(), 5);
    assert_eq!(results.by_ref().count(), 5);
    assert!(results.next().is_none());
    assert!(results.next_back().is_none());
}

#[test]
fn index_can_own_borrowed_consumer_values() {
    let name = String::from("outside");
    let mut builder = Builder::new();
    builder.watch(name.as_str(), [span(0, 1)]);
    let index = builder.build();
    let mut scratch = index.scratch();
    assert_eq!(
        *index
            .query([span(0, 1)], &mut scratch)
            .next()
            .unwrap()
            .value,
        "outside"
    );
}

#[test]
fn query_panic_does_not_poison_next_batch() {
    let mut builder = Builder::new();
    builder.watch((), [span(0, 1)]);
    let index = builder.build();
    let mut scratch = index.scratch();
    let failed = catch_unwind(AssertUnwindSafe(|| {
        let changes = (0..2).map(|i| {
            assert_eq!(i, 0, "intentional iterator panic");
            span(0, 1)
        });
        index.query(changes, &mut scratch).count();
    }));
    assert!(failed.is_err());
    assert_eq!(index.query([span(0, 1)], &mut scratch).len(), 1);
}

#[test]
fn builder_panic_keeps_owner_indices_valid() {
    let mut builder = Builder::new();
    let failed = catch_unwind(AssertUnwindSafe(|| {
        builder.watch(
            "partial",
            (0..2).map(|i| {
                assert_eq!(i, 0, "intentional iterator panic");
                span(0, 1)
            }),
        );
    }));
    assert!(failed.is_err());
    builder.watch("after", [span(0, 1)]);
    let index = builder.build();
    let mut scratch = index.scratch();
    let mut values: Vec<_> = index
        .query([span(0, 1)], &mut scratch)
        .map(|h| *h.value)
        .collect();
    values.sort_unstable();
    assert_eq!(values, ["after", "partial"]);
}

#[test]
fn shared_index_with_independent_worker_scratch() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<spanwatch::Index<String>>();
    send_sync::<Scratch>();
    let mut builder = Builder::new();
    for i in 0..100 {
        builder.watch(i, [span(i * 10, i * 10 + 5)]);
    }
    let index = builder.build();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let index = &index;
                scope.spawn(move || {
                    let mut scratch = index.scratch();
                    for i in 0..100 {
                        let start = ((i + worker) % 100) * 10;
                        let hits: Vec<_> = index
                            .query([span(start, start + 1)], &mut scratch)
                            .collect();
                        assert_eq!(hits.len(), 1);
                        assert_eq!(*hits[0].value, start / 10);
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
    });
}

#[test]
fn exhaustive_small_domain_batches_match_independent_oracle() {
    let spans: Vec<_> = (0..8)
        .flat_map(|a| (a + 1..=8).map(move |b| span(a, b)))
        .collect();
    let mut builder = Builder::new();
    for (id, &s) in spans.iter().enumerate() {
        builder.watch(id, [s]);
    }
    let index = builder.build();
    let mut scratch = index.scratch();
    for &a in &spans {
        for &b in &spans {
            let mut actual: Vec<_> = index
                .query([a, b], &mut scratch)
                .map(|h| {
                    let earliest =
                        if spans[*h.value].start() < a.end() && a.start() < spans[*h.value].end() {
                            0
                        } else {
                            1
                        };
                    assert_eq!(h.cause.change_index, earliest);
                    *h.value
                })
                .collect();
            actual.sort_unstable();
            let expected: Vec<_> = spans
                .iter()
                .enumerate()
                .filter_map(|(id, s)| {
                    [a, b]
                        .iter()
                        .any(|c| s.start() < c.end() && c.start() < s.end())
                        .then_some(id)
                })
                .collect();
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn adversarial_nested_and_identical_starts() {
    for same_start in [false, true] {
        let mut builder = Builder::new();
        for i in 0..10_000 {
            builder.watch(i, [span(if same_start { 0 } else { i }, 20_000 - i)]);
        }
        let index = builder.build();
        let mut scratch = index.scratch();
        assert_eq!(
            index.query([span(10_000, 10_001)], &mut scratch).len(),
            10_000
        );
        assert_eq!(index.query([span(20_000, u64::MAX)], &mut scratch).len(), 0);
    }
}
