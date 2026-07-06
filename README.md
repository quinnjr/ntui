# ntui

[![CI](https://github.com/quinnjr/ntui/actions/workflows/ci.yml/badge.svg)](https://github.com/quinnjr/ntui/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ntui.svg)](https://crates.io/crates/ntui)
[![docs.rs](https://docs.rs/ntui/badge.svg)](https://docs.rs/ntui)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

An [Ink](https://github.com/vadimdemedes/ink)-style TUI library for Rust: build
fullscreen (or inline) terminal UIs out of components and hooks, with a
React-style retained fiber tree, flexbox layout (via [`taffy`](https://github.com/DioxusLabs/taffy)),
and minimal-diff terminal output (via [`crossterm`](https://github.com/crossterm-rs/crossterm)).

If you've used Ink for React/Node, the shape will be familiar: `#[component]`
functions that call hooks (`use_state`, `use_effect`, `use_input`, ...) and
return an `element!` tree of `View`/`Text` nodes; state changes trigger
re-renders; the engine reconciles, lays out, paints, and diffs against the
previous frame before writing the minimal set of changed cells to the
terminal.

## Features

- **Components + hooks**, not a widget library — `#[component]` functions and
  `use_state` / `use_effect` / `use_input` / `use_future` / `use_stream` /
  `use_context` / `use_terminal_size` / `use_scroll` / `use_scrollback` /
  `use_app`, mirroring React's hook rules (identity by call order).
- **`element!`**, a JSX-like macro for building `View`/`Text` trees with typed
  props, keyed children, and `#(...)` for iterators/fragments.
- **A real reconciler**, not a diff-and-repaint-everything loop: a retained
  fiber tree keyed like React's, `props_eq` short-circuiting for subtrees that
  didn't change, and a bounded re-render loop (mirroring React's max update
  depth) for cascading `use_state` updates.
- **Flexbox layout** via `taffy` — `flex_direction`, `gap`, `padding`,
  `margin`, `justify_content`, `align_items`, `flex_grow`, percentage/cell
  dimensions, borders.
- **Cell-diff painting** — every frame paints into an in-memory buffer, then
  only the changed cells are written to the terminal.
- **Two rendering modes**: fullscreen (`render`, alternate screen + raw mode)
  and inline (`render_inline`, commits finished output into the terminal's
  real scrollback while a live region redraws at the bottom — see
  [`use_scrollback`](#hooks-v1) and [`inline_chat.rs`](ntui/examples/inline_chat.rs)).
- **Deterministic testing** without a TTY — `testing::TestTerminal` drives the
  same engine headlessly, frame by frame, for assertions on rendered text and
  input handling.

## Installation

```toml
[dependencies]
ntui = "0.1"
```

Requires Rust 2024 edition (a recent stable toolchain).

## Quickstart

```rust
use ntui::{component, element, render, BorderStyle, Color, FlexDirection, KeyCode, Weight};

#[component]
fn Counter(hooks: &mut ntui::Hooks) -> ntui::Element {
    let count = hooks.use_state(|| 0i32);
    let app = hooks.use_app();
    let c = count.clone();
    hooks.use_input(move |ev, _| match ev.code {
        KeyCode::Up => c.update(|n| *n += 1),
        KeyCode::Down => c.update(|n| *n -= 1),
        KeyCode::Char('q') => app.exit(),
        _ => {}
    });
    element! {
        View(flex_direction: FlexDirection::Column, padding: 1, border_style: BorderStyle::Round) {
            Text(content: format!("count: {}", count.get()), weight: Weight::Bold)
            Text(content: "↑/↓ to change · q to quit", color: Color::DarkGrey)
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(Counter)).await
}
```

Run it: `cargo run --example counter` (from the `ntui/` crate directory, or
`cargo run --example counter -p ntui` from the workspace root).

See it verbatim at [`ntui/examples/counter.rs`](ntui/examples/counter.rs), and
more examples in the same directory:

- [`spinner.rs`](ntui/examples/spinner.rs) — `use_future` driving an animated
  spinner.
- [`list.rs`](ntui/examples/list.rs) — a keyed, growable/shrinkable list.
- [`demo.rs`](ntui/examples/demo.rs) — a minimal chat demo: a text input line
  and a streamed reply, spawned with `tokio::spawn` from inside an input
  handler.
- [`claude_code.rs`](ntui/examples/claude_code.rs) — a fuller Claude Code-style
  interface: welcome banner, `●` tool-call blocks with `⎿` results, an animated
  "thinking" spinner with elapsed time, a full-width bordered input with a
  blinking cursor, a scrollable transcript that auto-follows streaming output
  (PgUp/PgDn to scroll back), and interrupt-on-Esc / quit-on-double-Ctrl-C.
- [`inline_chat.rs`](ntui/examples/inline_chat.rs) — an **inline** chat that
  commits finished turns into the terminal's real scrollback (`render_inline` +
  `use_scrollback`) while a live region streams the reply at the bottom.

## Hooks (v1)

| Hook | Purpose |
|---|---|
| `use_state` | Owned state; setting schedules a re-render of this component |
| `use_effect` | Run on mount/deps-change; cleanup on unmount/deps-change |
| `use_input` | Receive crossterm `KeyEvent`s routed to this component |
| `use_future` / `use_stream` | Spawn tokio work owned by the component; aborted on unmount |
| `use_context` / `ContextProvider` | Value injection down the tree |
| `use_terminal_size` | Reactive terminal dimensions |
| `use_scroll` | Scroll position for an `Overflow::Scroll` view; auto-follows the bottom |
| `use_scrollback` | Commit finished output into the terminal's real scrollback (inline mode) |
| `use_app` | App handle: `exit()`, request redraw |

## Development

```bash
cargo test --workspace                                   # full suite (lib tests + macro tests + doctests)
cargo clippy --workspace --all-targets -- -D warnings    # lint gate
cargo fmt --all -- --check                               # formatting gate
cargo build --examples -p ntui                           # compile-check examples
cargo bench -p ntui --features bench                     # engine benchmarks
```

CI (`.github/workflows/ci.yml`) runs the above on stable and nightly with
`RUSTFLAGS=-D warnings`, plus fuzz smoke tests (`wrap_text`, `truncate_line`,
`render_text`) from the standalone `fuzz/` cargo-fuzz workspace. See
[`CLAUDE.md`](CLAUDE.md) for the render pipeline architecture and codebase
conventions.

## v1 limitations

- **Two rendering modes.** `render` runs fullscreen (alternate screen + raw
  mode); `render_inline` runs inline and commits finished output into the
  terminal's real scrollback via `use_scrollback`, with a live region redrawn
  at the bottom. Inline mode assumes a small live region (input + a few lines);
  a very tall live region that exceeds the screen can glitch on redraw.
- **Char-width text measurement.** Text is measured at 1 column per `char`,
  not by grapheme cluster or display width — wide (e.g. CJK) and combining
  characters will misalign. Fix planned post-v1 via `unicode-width`.
- **No mouse support.** Only keyboard input is routed to components.
- **Context staleness.** `use_context` reads the nearest `ContextProvider`
  value at render time; because reconciliation is synchronous per frame,
  a provider update and a consumer's re-render are consistent within a single
  frame, but consumers that skip re-rendering (props-equal fast path) will not
  observe a context change until something else dirties them.

## Design

Full design rationale, the fiber/reconciler/layout/paint pipeline, and the
`Backend` trait are documented in
[`docs/superpowers/specs/2026-07-02-ntui-design.md`](docs/superpowers/specs/2026-07-02-ntui-design.md).
See also [`docs/`](docs/) for an architecture guide and hooks reference, also
published at <https://quinnjr.github.io/ntui/> (source in [`web/`](web/)).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
