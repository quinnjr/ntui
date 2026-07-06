# Hooks reference

All hooks are methods on `&mut ntui::Hooks`, handed to every component
render. Hook **identity is call order** — the same rule React follows: call
hooks unconditionally, in the same order, on every render. Calling more hooks
than the previous render panics (`next_slot`); calling fewer panics too
(`render_fiber`). Don't call hooks inside `if`/loops/after early returns.

## `use_state`

```rust
fn use_state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T>
```

Owned, per-fiber state. `init` runs once, on first mount. The returned
`State<T>` is `Clone` (cheap — it's a handle) and `Send` if `T: Send`, so it
can be moved into `use_input` closures and `tokio::spawn`ed tasks.

```rust
let count = hooks.use_state(|| 0i32);
count.set(5);
count.update(|n| *n += 1);
let current = count.get(); // requires T: Clone
```

`set`/`update` mark the owning fiber dirty, scheduling a re-render. Locking
recovers from poisoning, so a panic inside an `update` closure can't
permanently brick the state cell.

## `use_effect`

```rust
fn use_effect<D: PartialEq + 'static, C: Into<Cleanup>>(
    &mut self,
    deps: D,
    effect: impl FnOnce() -> C + 'static,
)
```

Runs `effect` after mount, and again whenever `deps` changes (compared with
`PartialEq`) between renders. Return `()` for no cleanup, or any `FnOnce()`
(via `Into<Cleanup>`) to run before the next effect invocation or on unmount.

```rust
hooks.use_effect(id.clone(), move || {
    let sub = subscribe(&id);
    move || sub.cancel() // Cleanup
});
```

## `use_input`

```rust
fn use_input(&mut self, handler: impl FnMut(KeyEvent, &mut InputCtx) + 'static)
```

Registers a handler for crossterm `KeyEvent`s routed to this component.
`InputCtx::stop_propagation()` prevents the event from reaching handlers
registered by ancestor components.

```rust
hooks.use_input(move |ev, ctx| match ev.code {
    KeyCode::Esc => { ctx.stop_propagation(); app.exit(); }
    _ => {}
});
```

## `use_future` / `use_stream`

```rust
fn use_future<Fut: Future<Output = ()> + Send + 'static>(&mut self, make: impl FnOnce() -> Fut)
fn use_stream<S: Stream + Send + 'static>(
    &mut self,
    make: impl FnOnce() -> S + Send + 'static,
    on_item: impl FnMut(S::Item) + Send + 'static,
)
```

Spawns tokio work owned by the component (`tokio::spawn`), aborted
automatically on unmount. The future/stream runs on the (`Send`) task, so it
can only talk back to the component through `State<T>` handles cloned in
before the `move` — never through borrows, since the fiber tree itself is
`!Send`. `use_stream` is sugar over `use_future` that polls a `Stream` and
calls `on_item` per item.

```rust
let f = frame.clone();
hooks.use_future(move || async move {
    loop {
        tokio::time::sleep(Duration::from_millis(120)).await;
        f.update(|n| *n = n.wrapping_add(1));
    }
});
```

## `use_context` / `ContextProvider`

```rust
fn use_context<T: 'static>(&mut self) -> Option<Rc<T>>
```

Reads the nearest ancestor `ContextProvider` value for `T`, if any, provided
via a `Provider` element in the tree above. Read at render time: because
reconciliation is synchronous per frame, a provider update and a consumer's
re-render are consistent within a single frame — but a consumer that skips
re-rendering entirely (its own props-equal fast path) won't observe a context
change until something else marks it dirty.

## `use_terminal_size`

```rust
fn use_terminal_size(&mut self) -> (u16, u16)
```

Reactive `(columns, rows)`; the component re-renders on terminal resize.

## `use_scroll`

```rust
fn use_scroll(&mut self) -> Scroll
```

A scroll position for an `Overflow::Scroll` `View` — pass a clone to that
view's `scroll` prop. Layout feeds content/viewport heights back into the
handle each frame, so the following methods stay clamped:

- `offset()` / `max_offset()` / `at_bottom()`
- `scroll_by(delta: i32)` — relative (e.g. PgUp = `-5`, PgDn = `5`)
- `scroll_to(offset: u16)`, `to_top()`, `to_bottom()`

The view **follows new content** (stays pinned to the bottom) whenever it's
already scrolled to the bottom — the behavior a chat transcript wants, so
streaming replies auto-scroll but a user who's scrolled back to read history
isn't yanked back down.

## `use_scrollback`

```rust
fn use_scrollback(&mut self) -> Scrollback
```

Only meaningful under `render_inline` (see
[`architecture.md`](architecture.md#two-rendering-modes)). `Scrollback::commit(element)`
prints `element` permanently above the live region — it scrolls into the
terminal's real, mouse-scrollable history. Committed elements should be
static (plain `View`/`Text`, no hooks/state — they render once). The typical
pattern: a chat commits each finished turn and drops it from live state,
keeping the live region to just the input/spinner.

Under fullscreen `render`, commits are queued but never drawn.

## `use_app`

```rust
fn use_app(&mut self) -> AppHandle
```

`AppHandle::exit()` stops the render loop and returns from `render`/`render_inline`.
`AppHandle::redraw()` requests a redraw without changing any state (rarely
needed — state changes already trigger redraws).

## Testing hooks

`RuntimeHandle::test_handle()` gives a fiber tree + wake receiver without a
full runtime loop, for unit-testing a hook in isolation. For anything
spanning render → input → frame, prefer `ntui::testing::TestTerminal`
(see [`getting-started.md`](getting-started.md#testing-without-a-tty)). Async
hook tests typically use `#[tokio::test(start_paused = true)]` for
deterministic, paused-clock time control over `use_future`/`use_stream`
timers.
