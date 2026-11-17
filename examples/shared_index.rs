//! Share a compiled index across workers with independent query memory.
use spanwatch::{Builder, InvalidSpan, Span};
use std::sync::Arc;

fn main() -> Result<(), InvalidSpan> {
    let mut builder = Builder::new();
    for page in 0..8 {
        builder.watch(
            format!("page-{page}"),
            [Span::new(page * 4096, (page + 1) * 4096)?],
        );
    }
    let index = Arc::new(builder.build());
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            let index = Arc::clone(&index);
            std::thread::spawn(move || {
                let mut scratch = index.scratch();
                let write = Span::new(worker * 4096 + 10, worker * 4096 + 20).unwrap();
                let hits: Vec<_> = index.query([write], &mut scratch).collect();
                assert_eq!(hits.len(), 1);
                println!("worker {worker}: {}", hits[0].value);
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker panicked");
    }
    Ok(())
}
