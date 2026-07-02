//! Criterion benchmarks for ntui's per-frame hot paths.
//!
//! Run with: `cargo bench -p ntui --features bench`
//!
//! Three tiers:
//! - `buffer_diff`  — the cell-grid frame diff (public `Buffer` API).
//! - `text_wrap`    — word wrapping / truncation (via the internal `__bench` surface).
//! - `mount` / `frame_reorder` / `mount_deep` — the full pipeline
//!   (reconcile → layout → paint → diff) driven through `TestTerminal`.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use ntui::__private::{truncate_line, wrap_text};
use ntui::buffer::{Buffer, Cell};
use ntui::testing::TestTerminal;
use ntui::{Color, FlexDirection, KeyCode, component, element};

// --- Tier 1: Buffer diff -------------------------------------------------

fn filled(w: u16, h: u16, ch: char) -> Buffer {
    let mut b = Buffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            b.set(
                x,
                y,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
    }
    b
}

fn bench_buffer_diff(c: &mut Criterion) {
    let (w, h) = (120u16, 40u16);
    let base = filled(w, h, 'a');
    let identical = base.clone();
    let full_change = filled(w, h, 'b');
    let mut quarter = base.clone();
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            quarter.set(
                x,
                y,
                Cell {
                    ch: 'b',
                    ..Cell::default()
                },
            );
        }
    }

    let mut g = c.benchmark_group("buffer_diff");
    g.bench_function("no_change", |b| {
        b.iter(|| black_box(identical.diff(black_box(&base))))
    });
    g.bench_function("quarter_change", |b| {
        b.iter(|| black_box(quarter.diff(black_box(&base))))
    });
    g.bench_function("full_change", |b| {
        b.iter(|| black_box(full_change.diff(black_box(&base))))
    });
    g.finish();
}

// --- Tier 2: Text wrapping ----------------------------------------------

fn bench_text(c: &mut Criterion) {
    let paragraph = "the quick brown fox jumps over the lazy dog and then keeps \
                     running across the wide green meadow under a bright morning sky"
        .to_string();
    let long_word = "a".repeat(200);

    let mut g = c.benchmark_group("text_wrap");
    g.bench_function("wrap_paragraph_40", |b| {
        b.iter(|| black_box(wrap_text(black_box(&paragraph), 40)))
    });
    g.bench_function("wrap_hardbreak_10", |b| {
        b.iter(|| black_box(wrap_text(black_box(&long_word), 10)))
    });
    g.bench_function("truncate", |b| {
        b.iter(|| black_box(truncate_line(black_box(&paragraph), 40)))
    });
    g.finish();
}

// --- Tier 3: full pipeline via TestTerminal -----------------------------

/// A keyed list whose order flips when 'r' is pressed — a stationary reorder
/// workload (reverse-of-reverse keeps the tree size constant across iterations).
#[derive(Clone, PartialEq, Default)]
struct ListProps {
    n: usize,
}

#[component]
fn List(props: &ListProps, hooks: &mut ntui::Hooks) -> ntui::Element {
    let rev = hooks.use_state(|| false);
    let r = rev.clone();
    hooks.use_input(move |ev, _| {
        if ev.code == KeyCode::Char('r') {
            r.update(|b| *b = !*b);
        }
    });
    let mut idx: Vec<usize> = (0..props.n).collect();
    if rev.get() {
        idx.reverse();
    }
    element! {
        View(flex_direction: FlexDirection::Column) {
            #(idx.into_iter().map(|i| element!(
                Text(key: i.to_string(), content: format!("item {i}"), color: Color::Cyan)
            )))
        }
    }
}

/// A chain of nested components with a context read at every level — stresses
/// per-render ancestor walks (context resolution) and mount recursion.
#[derive(Clone, PartialEq, Default)]
struct NestProps {
    depth: usize,
}

#[component]
fn Nest(props: &NestProps, hooks: &mut ntui::Hooks) -> ntui::Element {
    let _theme = hooks.use_context::<u32>();
    if props.depth == 0 {
        element!(Text(content: "leaf"))
    } else {
        element!(Nest(depth: props.depth - 1))
    }
}

fn bench_mount(c: &mut Criterion) {
    let mut g = c.benchmark_group("mount");
    for &n in &[50usize, 200, 500] {
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let t = TestTerminal::new(80, 50, element!(List(n: n))).unwrap();
                black_box(t.frame_text());
            });
        });
    }
    g.finish();
}

fn bench_frame_reorder(c: &mut Criterion) {
    let mut g = c.benchmark_group("frame_reorder");
    for &n in &[50usize, 200, 500] {
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let mut t = TestTerminal::new(80, 50, element!(List(n: n))).unwrap();
            b.iter(|| {
                t.send_key(KeyCode::Char('r')).unwrap();
                black_box(t.frame_text());
            });
        });
    }
    g.finish();
}

fn bench_mount_deep(c: &mut Criterion) {
    let mut g = c.benchmark_group("mount_deep");
    for &d in &[50usize, 150] {
        g.bench_with_input(BenchmarkId::from_parameter(d), &d, |b, &d| {
            b.iter(|| {
                let t = TestTerminal::new(
                    40,
                    10,
                    element!(ContextProvider(value: 7u32) { Nest(depth: d) }),
                )
                .unwrap();
                black_box(t.frame_text());
            });
        });
    }
    g.finish();
}

criterion_group!(
    benches,
    bench_buffer_diff,
    bench_text,
    bench_mount,
    bench_frame_reorder,
    bench_mount_deep
);
criterion_main!(benches);
