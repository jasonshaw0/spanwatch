//! Invalidate decoded fixed-layout packet views after writes to a mapped buffer.
use spanwatch::{Builder, InvalidSpan, Span};

#[derive(Debug)]
struct CachedView {
    name: &'static str,
    decoder: &'static str,
}

fn main() -> Result<(), InvalidSpan> {
    let mut builder = Builder::new();
    builder.watch(
        CachedView {
            name: "header",
            decoder: "decode_header",
        },
        [Span::new(0, 64)?],
    );
    builder.watch(
        CachedView {
            name: "payload",
            decoder: "decode_payload",
        },
        [Span::new(64, 512)?],
    );
    builder.watch(
        CachedView {
            name: "checksum",
            decoder: "verify_checksum",
        },
        [Span::new(0, 64)?, Span::new(64, 512)?],
    );
    let index = builder.build();
    let mut scratch = index.scratch();
    // Fixed-coordinate writes in one buffer revision. Rebuild if the layout changes.
    let writes = [Span::new(8, 12)?, Span::new(100, 120)?];
    for hit in index.query(writes, &mut scratch) {
        println!(
            "invalidate {} ({}): write #{} overlaps {:?}",
            hit.value.name, hit.value.decoder, hit.cause.change_index, hit.cause.overlap
        );
    }
    Ok(())
}
