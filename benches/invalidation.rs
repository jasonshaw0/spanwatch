//! Equivalent consumer-level invalidation on three lookup implementations.
use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use intervaltree::IntervalTree;
use spanwatch::{Builder, Cause, Index, Span};

fn span(start: u64, end: u64) -> Span {
    Span::new(start, end).unwrap()
}

struct Fixture {
    index: Index<usize>,
    dependencies: Vec<(Span, usize)>,
    alternative: IntervalTree<u64, usize>,
    changes: Vec<Span>,
}

impl Fixture {
    fn new(owners: usize, shape: &str) -> Self {
        let mut builder = Builder::new();
        let mut dependencies = Vec::new();
        for owner in 0..owners {
            let spans: Vec<_> = if shape == "nested" {
                vec![span(owner as u64, (owners * 2 - owner) as u64)]
            } else {
                (0..4)
                    .map(|slot| {
                        span(
                            (owner * 64 + slot * 8) as u64,
                            (owner * 64 + slot * 8 + 4) as u64,
                        )
                    })
                    .collect()
            };
            dependencies.extend(spans.iter().map(|&s| (s, owner)));
            builder.watch(owner, spans);
        }
        let alternative = dependencies
            .iter()
            .enumerate()
            .map(|(id, &(s, _))| (s.start()..s.end(), id))
            .collect();
        let changes = match shape {
            "sparse" => (0..16)
                .map(|i| {
                    let owner = (i * 1_103_515_245usize + 12345) % owners;
                    span((owner * 64 + 1) as u64, (owner * 64 + 2) as u64)
                })
                .collect(),
            "dense" => vec![span(0, (owners * 64 + 64) as u64)],
            "broad" => vec![span(
                (owners * 64 / 20) as u64,
                (owners * 64 * 19 / 20) as u64,
            )],
            "repeated" => vec![span((owners * 32 + 1) as u64, (owners * 32 + 2) as u64); 64],
            "nested" => vec![span(owners as u64, owners as u64 + 1)],
            _ => unreachable!(),
        };
        Self {
            index: builder.build(),
            dependencies,
            alternative,
            changes,
        }
    }
}

// Both alternatives perform the same stamp, output and exact-witness work as
// spanwatch. Setup, allocation, and validation are outside timed query loops.
struct Baseline {
    stamps: Vec<u64>,
    generation: u64,
    hits: Vec<(usize, Cause)>,
}

impl Baseline {
    fn new(owners: usize) -> Self {
        Self {
            stamps: vec![0; owners],
            generation: 0,
            hits: Vec::with_capacity(owners),
        }
    }

    fn begin(&mut self) {
        self.hits.clear();
        self.generation = match self.generation.checked_add(1) {
            Some(next) => next,
            None => {
                self.stamps.fill(0);
                1
            }
        };
    }

    fn report(&mut self, owner: usize, dependency: Span, change: Span, change_index: usize) {
        if self.stamps[owner] != self.generation {
            self.stamps[owner] = self.generation;
            self.hits.push((
                owner,
                Cause {
                    change_index,
                    change,
                    dependency,
                    overlap: dependency.intersection(change).unwrap(),
                },
            ));
        }
    }

    fn linear(&mut self, fixture: &Fixture) -> usize {
        self.begin();
        for (i, &change) in fixture.changes.iter().enumerate() {
            for &(dependency, owner) in &fixture.dependencies {
                if dependency.overlaps(change) {
                    self.report(owner, dependency, change, i);
                }
            }
        }
        black_box(&self.hits);
        self.hits.len()
    }

    fn intervaltree(&mut self, fixture: &Fixture) -> usize {
        self.begin();
        for (i, &change) in fixture.changes.iter().enumerate() {
            for hit in fixture.alternative.query(change.start()..change.end()) {
                let (dependency, owner) = fixture.dependencies[hit.value];
                self.report(owner, dependency, change, i);
            }
        }
        black_box(&self.hits);
        self.hits.len()
    }
}

fn queries(c: &mut Criterion) {
    for shape in ["sparse", "dense", "broad", "repeated", "nested"] {
        let mut group = c.benchmark_group(shape);
        group
            .sample_size(30)
            .warm_up_time(Duration::from_millis(100))
            .measurement_time(Duration::from_millis(500));
        for owners in [256, 4096, 65_536] {
            let fixture = Fixture::new(owners, shape);
            let mut scratch = fixture.index.scratch();
            let mut linear = Baseline::new(owners);
            let mut alternative = Baseline::new(owners);
            let mut expected: Vec<_> = fixture
                .index
                .query(fixture.changes.iter().copied(), &mut scratch)
                .map(|hit| hit.registration)
                .collect();
            expected.sort_unstable();
            linear.linear(&fixture);
            alternative.intervaltree(&fixture);
            for baseline in [&linear, &alternative] {
                let mut actual: Vec<_> = baseline.hits.iter().map(|&(owner, _)| owner).collect();
                actual.sort_unstable();
                assert_eq!(expected, actual);
            }
            group.bench_with_input(BenchmarkId::new("spanwatch", owners), &fixture, |b, f| {
                b.iter(|| {
                    let hits = black_box(&f.index)
                        .query(black_box(f.changes.iter().copied()), &mut scratch);
                    black_box(hits).len()
                });
            });
            group.bench_with_input(BenchmarkId::new("linear", owners), &fixture, |b, f| {
                b.iter(|| linear.linear(black_box(f)));
            });
            group.bench_with_input(
                BenchmarkId::new("intervaltree", owners),
                &fixture,
                |b, f| {
                    b.iter(|| alternative.intervaltree(black_box(f)));
                },
            );
        }
        group.finish();
    }
}

fn setup(c: &mut Criterion) {
    let mut group = c.benchmark_group("setup");
    group
        .sample_size(30)
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(500));
    for owners in [256, 4096, 65_536] {
        let fixture = Fixture::new(owners, "sparse");
        group.bench_function(BenchmarkId::new("build", owners), |b| {
            b.iter(|| {
                let mut builder = Builder::new();
                for owner in 0..owners {
                    builder.watch(
                        owner,
                        (0..4).map(|slot| {
                            let start = (owner * 64 + slot * 8) as u64;
                            span(start, start + 4)
                        }),
                    );
                }
                black_box(builder.build());
            })
        });
        group.bench_function(BenchmarkId::new("scratch", owners), |b| {
            b.iter(|| {
                black_box(black_box(&fixture.index).scratch());
            })
        });
    }
    group.finish();
}

criterion_group!(benches, queries, setup);
criterion_main!(benches);
