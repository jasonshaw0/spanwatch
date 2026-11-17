//! Generated differential tests against two independent lookup paths.
use intervaltree::IntervalTree;
use proptest::prelude::*;
use spanwatch::{Builder, Span};

fn normalized(a: u64, b: u64) -> Span {
    if a == b {
        if a == u64::MAX {
            Span::new(a - 1, a).unwrap()
        } else {
            Span::new(a, a + 1).unwrap()
        }
    } else {
        Span::new(a.min(b), a.max(b)).unwrap()
    }
}

fn config() -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 1024;
    }
    config
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn batches_match_linear_and_intervaltree(
        declarations in prop::collection::vec(
            prop::collection::vec((any::<u64>(), any::<u64>()), 0..8), 0..32),
        changes in prop::collection::vec((any::<u64>(), any::<u64>()), 0..24),
    ) {
        let declarations: Vec<Vec<_>> = declarations.into_iter()
            .map(|spans| spans.into_iter().map(|(a,b)| normalized(a,b)).collect()).collect();
        let changes: Vec<_> = changes.into_iter().map(|(a,b)| normalized(a,b)).collect();
        let mut builder = Builder::new();
        for (owner, spans) in declarations.iter().enumerate() {
            builder.watch(owner, spans.iter().copied());
        }
        let index = builder.build();
        let alternative: IntervalTree<_, _> = declarations.iter().enumerate()
            .flat_map(|(owner, spans)| spans.iter().map(move |s| (s.start()..s.end(), owner))).collect();
        let mut expected = Vec::new();
        for (owner, spans) in declarations.iter().enumerate() {
            if spans.iter().any(|s| changes.iter().any(|c| s.start() < c.end() && c.start() < s.end())) {
                expected.push(owner);
            }
        }
        let mut alternate: Vec<_> = changes.iter()
            .flat_map(|c| alternative.query(c.start()..c.end()).map(|hit| hit.value)).collect();
        alternate.sort_unstable();
        alternate.dedup();
        prop_assert_eq!(&expected, &alternate);
        let mut scratch = index.scratch();
        for batch in [&changes[..], &[][..], &changes[..]] {
            let hits: Vec<_> = index.query(batch.iter().copied(), &mut scratch).collect();
            let mut actual: Vec<_> = hits.iter().map(|h| h.registration).collect();
            actual.sort_unstable();
            if batch.is_empty() { prop_assert!(actual.is_empty()); }
            else { prop_assert_eq!(&actual, &expected); }
            for hit in hits {
                prop_assert_eq!(*hit.value, hit.registration);
                prop_assert!(declarations[hit.registration].contains(&hit.cause.dependency));
                let earliest = batch.iter().position(|c| declarations[hit.registration].iter()
                    .any(|s| s.start() < c.end() && c.start() < s.end())).unwrap();
                prop_assert_eq!(hit.cause.change_index, earliest);
                prop_assert_eq!(hit.cause.change, batch[earliest]);
                prop_assert_eq!(hit.cause.overlap.start(), hit.cause.change.start().max(hit.cause.dependency.start()));
                prop_assert_eq!(hit.cause.overlap.end(), hit.cause.change.end().min(hit.cause.dependency.end()));
                prop_assert!(hit.cause.overlap.start() < hit.cause.overlap.end());
            }
        }
        let mut reversed: Vec<_> = index.query(changes.iter().rev().copied(), &mut scratch)
            .map(|h| h.registration).collect();
        reversed.sort_unstable();
        prop_assert_eq!(reversed, expected);
    }

    #[test]
    fn narrow_ranges_exercise_sparse_branches(
        declarations in prop::collection::vec((0u64..512, 1u64..32, 0usize..16), 0..160),
        changes in prop::collection::vec((0u64..544, 1u64..16), 0..32),
    ) {
        let mut builder = Builder::new();
        for owner in 0..16 {
            builder.watch(owner, declarations.iter().filter(|&&(_,_,id)| id == owner)
                .map(|&(start,len,_)| Span::new(start,start+len).unwrap()));
        }
        let index = builder.build();
        let changes: Vec<_> = changes.iter().map(|&(start,len)| Span::new(start,start+len).unwrap()).collect();
        let mut expected: Vec<_> = declarations.iter().filter_map(|&(start,len,owner)| {
            changes.iter().any(|c| start < c.end() && c.start() < start+len).then_some(owner)
        }).collect();
        expected.sort_unstable(); expected.dedup();
        let mut scratch = index.scratch();
        let mut actual: Vec<_> = index.query(changes, &mut scratch).map(|h| h.registration).collect();
        actual.sort_unstable();
        prop_assert_eq!(actual, expected);
    }
}
