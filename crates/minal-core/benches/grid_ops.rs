//! Grid operation benchmarks.
//!
//! Measures scroll, resize, cell write, and snapshot performance.

use criterion::{Criterion, criterion_group, criterion_main};
use minal_core::grid::Grid;
use minal_core::term::Terminal;
use minal_core::term::TerminalSnapshot;

fn bench_scroll_up(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_scroll_up");

    for &(rows, cols) in &[(24, 80), (50, 120), (80, 200)] {
        group.bench_function(format!("{rows}x{cols}"), |b| {
            let mut grid = Grid::new(rows, cols);
            b.iter(|| {
                grid.scroll_up(0, rows, 1);
            });
        });
    }

    group.finish();
}

fn bench_scroll_down(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_scroll_down");

    for &(rows, cols) in &[(24, 80), (50, 120)] {
        group.bench_function(format!("{rows}x{cols}"), |b| {
            let mut grid = Grid::new(rows, cols);
            b.iter(|| {
                grid.scroll_down(0, rows, 1);
            });
        });
    }

    group.finish();
}

fn bench_input_char(c: &mut Criterion) {
    c.bench_function("input_char_80x24", |b| {
        let mut term = Terminal::new(24, 80);
        b.iter(|| {
            term.input_char('A');
        });
    });
}

fn bench_snapshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");

    for &(rows, cols) in &[(24, 80), (50, 120), (80, 200)] {
        group.bench_function(format!("{rows}x{cols}"), |b| {
            let term = Terminal::new(rows, cols);
            b.iter(|| {
                term.snapshot();
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_scroll_up,
    bench_scroll_down,
    bench_input_char,
    bench_snapshot
);
criterion_main!(benches);
