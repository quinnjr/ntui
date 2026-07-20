# Getting started

## Install

```toml
[dependencies]
ntui = "0.2"
tokio = { version = "1", features = ["rt", "macros"] }
```

It requires the Rust 2024 edition (a recent stable toolchain).

## Your first component

A component is a function that takes `&mut ntui::Hooks` and returns an
`ntui::Element`, annotated with `#[component]`:

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

`render()`'s future is `!Send` (fibers hold `Rc`s), so `main` must use the
single-threaded tokio runtime — `#[tokio::main(flavor = "current_thread")]`,
as above.

Run this exact component with `cargo run --example counter` from the `ntui/`
crate directory (or `-p ntui` from the workspace root). More runnable
examples live in [`ntui/examples/`](../ntui/examples):

| Example | Demonstrates |
|---|---|
| [`counter.rs`](../ntui/examples/counter.rs) | The component above |
| [`spinner.rs`](../ntui/examples/spinner.rs) | `use_future` driving an animation |
| [`list.rs`](../ntui/examples/list.rs) | A keyed, growable/shrinkable list |
| [`demo.rs`](../ntui/examples/demo.rs) | A minimal chat: input line + streamed reply via `tokio::spawn` |
| [`claude_code.rs`](../ntui/examples/claude_code.rs) | A fuller chat UI: tool-call blocks, spinner with elapsed time, scrollable transcript, interrupt/quit keys |
| [`inline_chat.rs`](../ntui/examples/inline_chat.rs) | `render_inline` + `use_scrollback`: committing finished turns into real terminal scrollback |

## What happens each frame

At a glance (full detail in [`architecture.md`](architecture.md)):

1. Something calls `State::set`/`update` — this marks the owning component
   dirty and wakes the event loop.
2. Dirty components re-render (your function body runs again); the
   reconciler diffs the returned `Element` tree against the retained fiber
   tree, matching children by key.
3. If layout-affecting props changed, `taffy` recomputes the flexbox layout.
4. The tree paints into an in-memory cell buffer, which is diffed against the
   previous frame, and only the changed cells are written to the terminal.

## Testing without a TTY

`ntui::testing::TestTerminal` drives the same engine headlessly:

```rust
use ntui::testing::TestTerminal;
use ntui::{element, KeyCode};

#[tokio::test]
async fn increments_on_up_arrow() {
    let mut term = TestTerminal::new(40, 10, element!(Counter)).unwrap();
    term.send_key(KeyCode::Up).unwrap();
    assert!(term.frame_text().contains("count: 1"));
}
```

This is how nearly all of `ntui`'s own tests work — see
[`hooks.md`](hooks.md) and the "Testing conventions" section of
[`CLAUDE.md`](../CLAUDE.md) for more.

## Next

- [`hooks.md`](hooks.md) for the full hook reference.
- [`architecture.md`](architecture.md) for the render pipeline and the two
  backends (fullscreen vs. inline).
