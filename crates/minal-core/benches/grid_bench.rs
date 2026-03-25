//! Benchmarks for terminal grid operations.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use minal_core::grid::Grid;
use minal_core::term::Terminal;

fn bench_grid_new(c: &mut Criterion) {
    c.bench_function("grid_new_80x24", |b| {
        b.iter(|| black_box(Grid::new(24, 80)));
    });
    c.bench_function("grid_new_200x50", |b| {
        b.iter(|| black_box(Grid::new(50, 200)));
    });
}

fn bench_grid_scroll(c: &mut Criterion) {
    let mut grid = Grid::new(24, 80);
    c.bench_function("grid_scroll_up_1", |b| {
        b.iter(|| {
            grid.scroll_up(0, 24, 1);
        });
    });
}

fn bench_dirty_check(c: &mut Criterion) {
    let grid = Grid::new(24, 80);
    c.bench_function("grid_has_any_dirty", |b| {
        b.iter(|| black_box(grid.has_any_dirty()));
    });

    let mut clean_grid = Grid::new(24, 80);
    clean_grid.clear_dirty();
    c.bench_function("grid_has_any_dirty_clean", |b| {
        b.iter(|| black_box(clean_grid.has_any_dirty()));
    });
}

fn bench_grid_clone(c: &mut Criterion) {
    let grid = Grid::new(24, 80);
    c.bench_function("grid_clone_80x24", |b| {
        b.iter(|| black_box(grid.clone()));
    });

    let large_grid = Grid::new(50, 200);
    c.bench_function("grid_clone_200x50", |b| {
        b.iter(|| black_box(large_grid.clone()));
    });
}

fn bench_terminal_snapshot(c: &mut Criterion) {
    let term = Terminal::new(24, 80);
    c.bench_function("terminal_snapshot_80x24", |b| {
        b.iter(|| black_box(term.snapshot()));
    });

    let large_term = Terminal::new(50, 200);
    c.bench_function("terminal_snapshot_200x50", |b| {
        b.iter(|| black_box(large_term.snapshot()));
    });
}

criterion_group!(
    benches,
    bench_grid_new,
    bench_grid_scroll,
    bench_dirty_check,
    bench_grid_clone,
    bench_terminal_snapshot,
);
criterion_main!(benches);
