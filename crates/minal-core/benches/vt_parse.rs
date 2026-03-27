//! VT parse throughput benchmark (vtebench-style).
//!
//! Measures bytes/sec of mixed VT output through the parser + terminal.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use minal_core::handler::Handler;
use minal_core::term::Terminal;

/// Build a ~1 MB buffer of mixed VT content:
/// plain text, SGR color escapes, and cursor movement.
fn build_vt_corpus() -> Vec<u8> {
    let mut buf = Vec::with_capacity(1_048_576);
    let line = b"Hello, world! This is a test line with some content.\r\n";
    let sgr = b"\x1b[38;5;196mRed text\x1b[0m ";
    let cup = b"\x1b[10;20H";

    while buf.len() < 1_000_000 {
        buf.extend_from_slice(line);
        buf.extend_from_slice(sgr);
        buf.extend_from_slice(cup);
    }
    buf
}

fn bench_vt_parse(c: &mut Criterion) {
    let corpus = build_vt_corpus();

    let mut group = c.benchmark_group("vt_parse");
    group.throughput(Throughput::Bytes(corpus.len() as u64));

    group.bench_function("parse_1mb", |b| {
        b.iter(|| {
            let mut term = Terminal::new(80, 120);
            let mut parser = vte::Parser::new();
            let mut handler = Handler::new(&mut term);
            for &byte in &corpus {
                parser.advance(&mut handler, byte);
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_vt_parse);
criterion_main!(benches);
