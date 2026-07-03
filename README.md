# ntui

[![CI](https://github.com/quinnjr/ntui/actions/workflows/ci.yml/badge.svg)](https://github.com/quinnjr/ntui/actions/workflows/ci.yml)

An Ink-style TUI library for Rust: build fullscreen terminal UIs out of
components and hooks, with a React-style retained fiber tree, flexbox layout
(via `taffy`), and minimal-diff terminal output (via `crossterm`).

If you've used [Ink](https://github.com/vadimdemedes/ink) for React/Node,
the shape will be familiar: `#[component]` functions that call hooks
(`use_state`, `use_effect`, `use_input`, ...) and return an `element!` tree of
`View`/`Text` nodes; state changes trigger re-renders; the engine reconciles,
lays out, paints, and diffs against the previous frame.

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
three more examples in the same directory:

- [`spinner.rs`](ntui/examples/spinner.rs) — `use_future` driving an animated
  spinner.
- [`list.rs`](ntui/examples/list.rs) — a keyed, growable/shrinkable list.
- [`demo.rs`](ntui/examples/demo.rs) — a Claude-Code-ish chat demo: a text
  input line and a streamed reply, spawned with `tokio::spawn` from inside an
  input handler.

## Hooks (v1)

| Hook | Purpose |
|---|---|
| `use_state` | Owned state; setting schedules a re-render of this component |
| `use_effect` | Run on mount/deps-change; cleanup on unmount/deps-change |
| `use_input` | Receive crossterm `KeyEvent`s routed to this component |
| `use_future` / `use_stream` | Spawn tokio work owned by the component; aborted on unmount |
| `use_context` / `ContextProvider` | Value injection down the tree |
| `use_terminal_size` | Reactive terminal dimensions |
| `use_app` | App handle: `exit()`, request redraw |

## v1 limitations

- **Fullscreen-only.** Only `FullscreenBackend` (alternate screen + raw mode)
  ships in v1. Inline/scrollback rendering and `<Static>` output are designed
  for at the `Backend` trait level but not implemented yet.
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
