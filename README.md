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
more examples in the same directory:

- [`spinner.rs`](ntui/examples/spinner.rs) — `use_future` driving an animated
  spinner.
- [`list.rs`](ntui/examples/list.rs) — a keyed, growable/shrinkable list.
- [`demo.rs`](ntui/examples/demo.rs) — a minimal chat demo: a text input line
  and a streamed reply, spawned with `tokio::spawn` from inside an input
  handler.
- [`claude_code.rs`](ntui/examples/claude_code.rs) — a fuller Claude Code-style
  interface: welcome banner, `●` tool-call blocks with `⎿` results, an animated
  "thinking" spinner with elapsed time, a bordered input with a blinking cursor,
  a scrollable transcript that auto-follows streaming output (PgUp/PgDn to
  scroll back), and interrupt-on-Esc.
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
