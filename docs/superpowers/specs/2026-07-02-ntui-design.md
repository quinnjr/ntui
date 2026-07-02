# ntui — an Ink-style TUI library for Rust

**Date:** 2026-07-02
**Status:** Approved design, pre-implementation

## Goal

A general-purpose, publishable Rust TUI library with the authoring style of
[Ink](https://github.com/vadimdemedes/ink) (React for CLIs): components as
functions, hooks for state and effects, a JSX-like macro, flexbox layout. The
motivating application is a Claude Code-like TUI, but the library is the
product; the app is a demo/driver.

## Decisions

| Decision | Choice |
|---|---|
| Positioning | Library-first, publishable crate |
| Foundations | `crossterm` (terminal I/O, events), `taffy` (flexbox layout) |
| Authoring style | `element!` macro + `#[component]` functions + hooks |
| Rendering mode (v1) | Fullscreen (alternate screen); inline/scrollback mode later behind the same backend trait |
| Async | Tokio built-in; hooks spawn runtime tasks tied to component lifetime |
| Engine | Full React-style reconciler (keyed element diffing over a retained fiber tree) |

## Workspace layout

```
ntui/            # core: engine, hooks, components, terminal backend
ntui-macros/     # proc macros: element!, #[component]
examples/        # spinner, counter, keyed list, claude-code-ish demo
```

A separate `ntui-components` crate (TextInput, Spinner, etc.) comes after the
core is stable; until then, reusable widgets live in `examples/`.

## Public API

### Entry point

```rust
ntui::render(element!(App)).await?;   // runs the event loop until exit
```

Returns `Result<(), ntui::Error>`.

### Authoring

```rust
#[component]
fn App(hooks: Hooks) -> impl Into<AnyElement> {
    let count = hooks.use_state(|| 0);
    let items = ["alpha", "beta", "gamma"];

    element! {
        View(flex_direction: Column, padding: 1, border_style: BorderStyle::Round) {
            Text(content: format!("count = {}", count.get()), weight: Weight::Bold)
            #(items.iter().map(|it| element!(Text(key: *it, content: *it))))
        }
    }
}
```

- `element!` is Rust's JSX: `Component(prop: value, ...) { children }`.
- `#(expr)` splices an iterator of elements (dynamic children); the `key`
  prop controls identity across renders.
- Props are plain structs deriving `Clone, PartialEq, Default`; every field
  must have a default so the macro can set any subset.

  > **Erratum:** `#[derive(Props)]` was dropped during implementation as YAGNI;
  > props derive `Clone, PartialEq, Default` directly rather than via a custom
  > derive.
- `Hooks` is a handle passed to every component function. Hook identity is
  call order (React's rules); violations panic at runtime with the component
  name and hook index.

### Built-in components (v1)

- **View** — flexbox container: `flex_direction`, `gap`, `padding`, `margin`,
  `width`/`height` (fixed, percent, auto), `border_style`, `background`.
- **Text** — `content`, `color`, `weight`, wrap/truncate behavior.
- **Fragment** — grouping without a layout node.

### Hooks (v1)

| Hook | Purpose |
|---|---|
| `use_state` | Owned state; setting schedules a re-render of this component |
| `use_effect` | Run on mount/deps-change; cleanup on unmount/deps-change |
| `use_input` | Receive crossterm `KeyEvent`s routed to this component |
| `use_future` / `use_stream` | Spawn tokio work owned by the component; aborted on unmount |
| `use_context` / `ContextProvider` | Value injection down the tree |
| `use_terminal_size` | Reactive terminal dimensions |
| `use_app` | App handle: `exit()`, request redraw |

## Engine internals

### Fiber tree

The engine retains a tree of `Fiber` nodes, one per mounted element. Each
fiber holds:

- component type id + current props
- hooks storage: `Vec<HookSlot>` indexed by call order
- its `taffy` layout node
- handles of tasks spawned via `use_future`/`use_stream`
- child fibers

Host components (`View`, `Text`, `Fragment`) are fibers with no hooks.

### Reconciliation

1. A state change marks its fiber dirty and wakes the event loop.
2. The dirty fiber's function re-runs, producing a new element subtree.
3. New children are diffed against existing child fibers React-style:
   - match by `key` when present, else by index + component type;
   - matched → update props in place; skip re-render when props are
     `PartialEq`-equal; recurse;
   - unmatched old → unmount: run effect cleanups, abort owned tasks, drop
     the taffy node;
   - unmatched new → mount.
4. Reconciliation runs synchronously per frame. No fiber
   scheduling/time-slicing — terminals do not need interruptible rendering.

### Layout and paint

- After reconciliation, `taffy` recomputes layout only if structure or
  layout-affecting props changed (dirty-flagged).
- Paint walks fibers depth-first into a `Buffer`: a grid of
  `Cell { char, fg, bg, attrs }`.
- The new buffer is diffed against the previous frame; minimal ANSI output
  (cursor moves + styled runs) is emitted through the backend.

### Backend trait

```rust
trait Backend {
    fn size(&self) -> (u16, u16);
    fn flush(&mut self, diff: BufferDiff) -> io::Result<()>;
    fn enter(&mut self) -> io::Result<()>;
    fn leave(&mut self) -> io::Result<()>;
}
```

v1 ships `FullscreenBackend` (crossterm alternate screen + raw mode). The
future inline backend implements the same trait plus a `Static` overflow
channel for scrollback output; nothing above the paint layer changes.

### Event loop

One tokio `select!` over:

- crossterm `EventStream` (keys, resize),
- the dirty-fiber wake channel,
- messages from hook-spawned tasks.

Input events route to fibers with `use_input` registrations, deepest-first,
with `stop_propagation()` as an escape hatch. Frames are coalesced: any number
of state changes between frames produce one render pass, capped at ~60 fps.

## Error handling

- `render()` returns `Result<(), ntui::Error>` (terminal I/O errors, etc.).
- A panic inside a component is caught at the loop boundary; the terminal is
  restored (raw mode off, alternate screen exited) before the panic resumes.
- Terminal restoration is additionally guaranteed by a `Drop` guard and a
  panic hook, so a crashed app never leaves the user's shell in raw mode.
- Hook-rule violations (count/order mismatch across renders) panic with the
  component name and hook index.

## Testing

1. **Reconciler unit tests** with a mock backend: assert mount/unmount order,
   effect cleanup sequences, keyed-list moves, task abortion on unmount.
2. **Snapshot tests** (`insta`): render components to a `Buffer`, assert the
   text grid.
3. **Integration harness** — `TestTerminal` feeds synthetic key events and
   asserts rendered frames. Ships as public API (`ntui::testing`) for library
   users.

## Out of scope for v1

- Inline/scrollback rendering mode and `<Static>` (designed for via the
  backend trait; implemented later).
- Widget library beyond View/Text/Fragment (`ntui-components`, later).
- Mouse support, non-tokio runtimes, Windows-legacy-console quirks beyond
  what crossterm handles.
