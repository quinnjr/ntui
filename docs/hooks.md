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

## `use_paste`

```rust
fn use_paste(&mut self, handler: impl FnMut(&str, &mut InputCtx) + 'static)
```

Registers a handler for bracketed-paste events (both `render` and
`render_inline` enable bracketed paste on enter). The pasted text arrives
whole, once per paste — unlike a paste on a terminal without bracketed
paste, which arrives as a burst of key events where each newline is
indistinguishable from an Enter press. Same deepest-first dispatch and
`stop_propagation()` semantics as `use_input`.

```rust
hooks.use_paste(move |text, _ctx| {
    input.update(|b| b.push_str(&text.replace('\n', " ")));
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

## `use_task`

```rust
fn use_task<D: PartialEq + 'static, Fut: Future<Output = ()> + Send + 'static>(
    &mut self,
    deps: D,
    make: impl FnOnce() -> Fut + 'static,
)
```

`use_future`'s deps-keyed sibling: spawns `make()` after commit, and whenever
`deps` changes between renders aborts the current task and spawns a fresh
one (also aborted on unmount). Use it when the spawned work is a function of
an input that can change — a timer keyed on a `Duration` prop, an animation
driver keyed on its target — so stale work stops instead of racing its
replacement. Same communication rules as `use_future`: talk back through
`State<T>` handles only.

```rust
let f = fired.clone();
hooks.use_task(duration, move || async move {
    let Some(d) = duration else { return };
    tokio::time::sleep(d).await;
    f.set(true);
});
```

## `use_interval` / `use_tween`

```rust
fn use_interval(&mut self, period: Duration, on_tick: impl FnMut() + Send + 'static)
fn use_tween(&mut self, target: f32, duration: Duration) -> f32
```

`use_interval` calls `on_tick` every `period` for as long as the component
stays mounted, starting after the first period elapses (one spawned task,
aborted on unmount). `use_tween` animates toward `target` (ease-out-cubic),
returning the current interpolated value each render; retargeting continues
smoothly from wherever the value currently is, and its internal ~60Hz driver
task runs only while the animation is in flight. If a timer closure panics,
the task dies silently and the timer stops — same caveat as `use_future`.

## `use_theme` / `use_focus_scope` / `use_focusable`

```rust
fn use_theme(&mut self) -> Theme
fn use_focus_scope(&mut self) -> FocusScopeHandle
fn use_focusable(&mut self) -> Focus
```

The widget layer's shared hooks (`ntui::widgets`). `use_theme` reads the
nearest provided [`Theme`] (falling back to the built-in default), so custom
components can match the first-party widgets' colors. `use_focus_scope`
creates a Tab/Shift-Tab focus registry — provide the returned handle via
`ContextProvider` — and `use_focusable` registers the calling component in
the nearest scope, reporting `is_focused()` and offering `claim()`. See the
widgets guide for the composition pattern.

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

## `use_memo`

```rust
fn use_memo<D, T>(&mut self, deps: D, f: impl FnOnce() -> T) -> T
where D: PartialEq + 'static, T: Clone + 'static
```

Computes `f` on the first render and again only when `deps` change.

A component re-renders for many reasons — a keystroke, a parent's state
change, a resize — and most of them do not affect any given derived value.
Without a memo, work proportional to the *input* runs on every one of those
renders even when the input has not moved.

Keep `deps` small: the point is to spend a cheap comparison instead of an
expensive recompute. For a large shared payload, depend on its identity
rather than its contents by wrapping it in [`Shared`](#shared) — and note
that a `Shared` left at its `Default` allocates a fresh pointer each render,
which defeats the comparison.

The value is cloned out on every render, so `T` should be cheap to clone.
Calling the hook with a different `D` or `T` at the same slot panics with
the component's name, like every other hook's order check.

```rust
let matches = hooks.use_memo(
    (props.rows.clone(), props.query.clone()),
    || Shared::new(filter(&props.rows, &props.query)),
);
```

## `use_list_selection`

```rust
fn use_list_selection(&mut self) -> State<ListSelection>
```

A cursor and viewport over a list, persisted across renders.

`ListSelection` is plain data — `index` and `offset` — with `move_by`,
`to_start`, `to_end`, `clamp`, and `visible`. Length and viewport height are
passed per call rather than stored, because both change independently of the
selection: the list is refreshed from elsewhere and the height follows the
terminal.

`clamp` is the one to remember. Lists refreshed from outside routinely shrink
under a cursor that was valid a moment ago; without it the cursor points past
the end and the viewport scrolls to blank space.

`visible` returns the range to render — slice with it *before* building any
row, so a frame costs what is on screen rather than what is in the list. It
pairs with `TableProps::viewport`.

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
`AppHandle::copy_to_clipboard(text)` asks the backend to set the system
clipboard via an OSC 52 sequence on the next frame — best-effort, since the
terminal emulator may ignore or gate it (tmux and some terminals require
opt-in). Note this is *programmatic* copy; mouse selection already works
without it because ntui never captures the mouse.

## Testing hooks

`RuntimeHandle::test_handle()` gives a fiber tree + wake receiver without a
full runtime loop, for unit-testing a hook in isolation. For anything
spanning render → input → frame, prefer `ntui::testing::TestTerminal`
(see [`getting-started.md`](getting-started.md#testing-without-a-tty)). Async
hook tests typically use `#[tokio::test(start_paused = true)]` for
deterministic, paused-clock time control over `use_future`/`use_stream`
timers.
