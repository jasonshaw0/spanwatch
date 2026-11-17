//! Isolated integration-test process: unrelated test allocations cannot interfere.
use spanwatch::{Builder, Span};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::{alloc::System, hint::black_box};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn prepared_queries_allocate_nothing_including_after_generation_changes() {
    let mut builder = Builder::new();
    for owner in 0..1024 {
        builder.watch(
            owner,
            [
                Span::new(owner * 8, owner * 8 + 4).unwrap(),
                Span::new(0, 1).unwrap(),
            ],
        );
    }
    let index = builder.build();
    let mut scratch = index.scratch();
    let changes = [Span::new(0, 8192).unwrap(), Span::new(1, 2).unwrap()];
    let region = Region::new(GLOBAL);
    for _ in 0..100 {
        assert_eq!(black_box(index.query(changes, &mut scratch)).len(), 1024);
        assert_eq!(black_box(index.query([], &mut scratch)).len(), 0);
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}
