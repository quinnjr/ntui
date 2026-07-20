# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

ntui is an Ink-style TUI library for Rust: components as functions + hooks, an `element!` JSX-like macro, a React-style keyed reconciler over a retained fiber tree, taffy flexbox layout, cell-diff painting, and a fullscreen crossterm backend on a tokio event loop. Workspace: `ntui` (the library) and `ntui-macros` (proc macros `element!` and `#[component]`).

Design spec: `docs/superpowers/specs/2026-07-02-ntui-design.md`. Two things originally out-of-scope there have since shipped: inline/scrollback rendering (`render_inline`, `backend/inline.rs`, `use_scrollback`) and a first-party widget layer (`ntui/src/widgets/`, built entirely from the five element kinds — no new node kind). The spec's remaining out-of-scope item (mouse) still stands — don't add it without being asked.

## Commands

```bash
cargo test --workspace                                  # full suite (lib tests + tests/macros.rs + doctests)
cargo test -p ntui <test_name>                          # single test by (partial) name
cargo test -p ntui --test macros                        # macro integration tests only
cargo clippy --workspace --all-targets -- -D warnings   # the lint gate — must stay clean
cargo clippy -p ntui --all-targets --all-features -- -D warnings  # also lints the bench/fuzz surface (__private)
cargo fmt --all -- --check                              # formatting gate (run `(cd fuzz && cargo fmt -- --check)` too)
cargo build --examples -p ntui                          # compile-check examples
cargo run --example counter|spinner|list|demo|claude_code|inline_chat  # run interactively (needs a real TTY)
cargo bench -p ntui --features bench                    # engine benchmarks (ntui/benches/engine.rs)
```

All of test/clippy(both invocations)/fmt(both invocations)/examples are CI gates (`.github/workflows/ci.yml`) and expected green on every commit. The clippy gate is `--all-targets` specifically — plain `cargo clippy` misses test-target lint failures. CI runs the full matrix on both stable and nightly with `RUSTFLAGS=-D warnings`.

Fuzzing lives in the separate `fuzz/` cargo-fuzz workspace (excluded from the main workspace; has its own `Cargo.lock`). Targets: `wrap_text`, `truncate_line`, `render_text` — each fuzzes a `pub` helper reachable only via the `fuzz` feature's `__private` module. Run e.g. `cd fuzz && cargo +nightly fuzz run wrap_text -- -max_total_time=10`.

## Architecture

The render pipeline, one frame:

```
State::set ──► Wake::Dirty on unbounded channel
  runtime::AppCore::process_wakes   drains channel, dedups, sorts dirty fibers shallowest-first,
                                    re-renders each (bounded by MAX_UPDATE_PASSES ≈ React's max update depth),
                                    then FiberTree::flush_effects
  reconciler::render_fiber          runs the component fn, then reconcile_children: match by key,
                                    else index+type; props_eq short-circuits whole subtrees
  layout::compute_layout            rebuilds a fresh TaffyTree from the fiber tree (only when
                                    tree.layout_dirty), text measured via wrap_text, writes Rect per fiber
  paint::paint                      DFS fibers → Buffer (grid of Cells); document order = z-order
  Buffer::diff                      cell-level diff vs previous frame
  Backend::flush                    minimal updates out (FullscreenBackend = crossterm; TestBackend = in-memory)
```

Key types and where they live:

- `element.rs`/`component.rs` — `Element`/`Node` (host View/Text/Fragment/Provider or type-erased `Box<dyn AnyComponent>`). Components implement `Component` (usually via `#[component]`).
- `fiber.rs` — retained `FiberTree` (HashMap arena, monotonic ids, never reused). Fibers own hook slots, children, layout Rect, `rendered_once`.
- `hooks/` — one file per hook. `Hooks` is handed to component fns; hook identity is call order, enforced two ways: `next_slot` panics on growth after first render, `render_fiber` panics on shrink.
- `runtime.rs` — `AppCore` (engine state machine, driver-agnostic) + `render()` (the real tokio select loop) + `RestoreGuard`/panic hook (terminal restore on every exit path).
- `testing.rs` — public `TestTerminal`: drives `AppCore` by hand for deterministic tests. This is how virtually all integration-style tests work; no TTY needed.

### Two rendering modes

`render()` (fullscreen) and `render_inline()` (inline) share `AppCore`/reconciler/layout/paint but diverge at the backend:

- **Fullscreen** (`backend/fullscreen.rs`): alternate screen + raw mode; `Buffer::diff` against the previous frame drives `FullscreenBackend::flush`.
- **Inline** (`backend/inline.rs`, driven by `run_inline_loop` in `runtime.rs`): no alternate screen. Each frame splits into *committed* rows (printed permanently via `InlineSink::commit`, scrolling into real terminal history) and a *live region* redrawn in place via `InlineSink::present`. `hooks/scrollback.rs`'s `Scrollback` (`use_scrollback`) is how a component pushes finished, static `Element`s into the committed side; outside inline mode those commits are queued but never drawn. `backend/ansi.rs` holds the cell→ANSI row writer shared by both backends.

`hooks/scroll.rs` (`use_scroll`) is unrelated to `Scrollback` — it's scroll *position* for an `Overflow::Scroll` view (auto-follows the bottom, `scroll_by` for PgUp/PgDn), independent of rendering mode.

## Invariants that bite

- **The `mem::take` hooks pattern** (`reconciler::render_fiber`, `fiber::flush_effects`): hook slots are taken out of the fiber while user code runs, then restored. Don't "fix" the borrow structure with RefCells; a panic mid-render intentionally drops that fiber's slots (documented; app tears down via RestoreGuard).
- **`layout_dirty` discipline**: `mount_element`/`unmount` set it; `reconcile_children` sets it only on pure reorder; `update_fiber` sets it only when host props actually changed. A no-op re-render must NOT set it (there's a test pinning this).
- **Backend contract**: `enter()` must leave the screen cleared (first frame diffs against blank) and must be self-cleaning on partial failure (raw mode must not leak). See trait docs in `backend/mod.rs`.
- **`render()`'s future is `!Send`** (Rc in fibers) — examples use `#[tokio::main(flavor = "current_thread")]`. Hook-spawned tasks ARE Send and talk back only through `State<T>` handles (Arc<Mutex>, poison-recovering).
- **Macro contracts**: `element!` resolves component props types by the `{Name}Props` naming convention and emits fully-qualified `::ntui::` paths; unsuffixed integer literal props are deliberately NOT wrapped in `Into::into` (inference falls back to i32 and breaks — see the comment in `ntui-macros/src/lib.rs`). `__`-prefixed identifiers are reserved inside `element!` expansions.
- **`#[allow(dead_code)]` on `FiberTree::len`/`kind_name` is load-bearing**: they're used only from `#[cfg(test)]` code, and the lib target's dead-code lint fires without the allows (verified). Don't remove them on a lint crusade.
- Engine modules (`fiber`, `reconciler`, `runtime`, `layout`, `paint`, `text`) are `pub(crate)`; the public surface is the curated re-export list in `lib.rs`. Keep it that way — anything re-exported is semver surface.

## Docs site

`web/` is an Astro + Tailwind site (`pnpm install && pnpm dev` from `web/`)
that renders `docs/*.md` via a content collection — see `web/README.md`. It
deploys to GitHub Pages (`.github/workflows/docs.yml`, triggered on `main`)
and is unrelated to the Rust workspace's build/test/lint gates above.

## Testing conventions

Unit tests live in `#[cfg(test)] mod tests` inside each source file and may use `pub(crate)` internals plus `test_util::Shared<T>` (Arc-pointer-equality wrapper for smuggling handles/logs out of components without defeating `props_eq`). Async hook tests use `#[tokio::test(start_paused = true)]` with paused-clock time control. `RuntimeHandle::test_handle()` gives a tree + wake receiver without a runtime loop. Prefer `TestTerminal` for anything spanning render→input→frame.
