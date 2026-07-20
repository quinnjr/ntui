use std::collections::HashSet;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::backend::inline::{InlineSink, buffer_rows};
use crate::backend::{Backend, FullscreenBackend, InlineBackend};
use crate::buffer::{Buffer, Cell};
use crate::element::Element;
use crate::error::Error;
use crate::fiber::{FiberId, FiberTree};
use crate::hooks::{RuntimeHandle, Wake};
use crate::layout::compute_layout;
use crate::paint::paint;

const FRAME: Duration = Duration::from_millis(16); // ~60fps cap

/// Per-frame bound on burst draining: caps how many immediately-ready input
/// events (paste, key repeat) are drained in a single iteration so wakes
/// can't be starved by a large burst.
const MAX_EVENT_BURST: usize = 128;

/// Cap on outer `process_wakes` fixpoint passes. Mirrors React's
/// "maximum update depth exceeded" guard: a component whose effect
/// re-dirties state on every pass would otherwise spin the loop forever.
const MAX_UPDATE_PASSES: usize = 64;

/// The engine loop's shared core. `render()` drives it with a select loop;
/// `testing::TestTerminal` drives it by hand for deterministic tests.
pub(crate) struct AppCore {
    pub tree: FiberTree,
    pub rt: RuntimeHandle,
    wake_rx: UnboundedReceiver<Wake>,
    pending: Vec<FiberId>,
    prev: Option<Buffer>,
    /// Spare buffer for `draw`'s double-buffering (see `draw`'s doc comment).
    scratch: Option<Buffer>,
    pub size: (u16, u16),
    pub exited: bool,
    /// Last `(width, max_height)` passed to `live_rows`, used to gate its
    /// `compute_layout` call the same way `draw` gates on `layout_dirty` /
    /// `prev.is_none()`. `None` until the first `live_rows` call.
    live_size: Option<(u16, u16)>,
}

impl AppCore {
    pub fn new(el: Element, size: (u16, u16)) -> Self {
        let (wake, wake_rx) = unbounded_channel();
        let rt = RuntimeHandle {
            wake,
            size: std::sync::Arc::new(std::sync::Mutex::new(size)),
            scrollback: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        };
        let mut tree = FiberTree::new();
        tree.mount_root(el, &rt);
        tree.flush_effects();
        AppCore {
            tree,
            rt,
            wake_rx,
            pending: Vec::new(),
            prev: None,
            scratch: None,
            size,
            exited: false,
            live_size: None,
        }
    }

    pub fn apply_wake(&mut self, w: Wake) {
        match w {
            Wake::Dirty(id) => self.pending.push(id),
            Wake::Redraw => {
                if let Some(root) = self.tree.root {
                    self.pending.push(root);
                }
                self.prev = None; // full repaint
            }
            Wake::Exit => self.exited = true,
        }
    }

    /// Drain the wake channel, re-render dirty fibers shallowest-first,
    /// then run effects (which may queue more wakes for the next pass).
    pub fn process_wakes(&mut self) {
        while let Ok(w) = self.wake_rx.try_recv() {
            self.apply_wake(w);
        }
        // Loop: effects may set state synchronously.
        let mut passes = 0;
        while !self.pending.is_empty() {
            passes += 1;
            if passes > MAX_UPDATE_PASSES {
                panic!(
                    "ntui: maximum update depth exceeded ({MAX_UPDATE_PASSES} passes) — a use_effect that sets state unconditionally, or a state update during every render, is preventing the UI from reaching a stable state"
                );
            }
            let mut dirty = std::mem::take(&mut self.pending);
            let mut seen = HashSet::new();
            dirty.retain(|id| seen.insert(*id));
            dirty.sort_by_key(|id| self.depth(*id));
            let rt = self.rt.clone();
            for id in dirty {
                self.tree.render_fiber(id, &rt); // no-ops if already unmounted
            }
            self.tree.flush_effects();
            while let Ok(w) = self.wake_rx.try_recv() {
                self.apply_wake(w);
            }
        }
    }

    /// Route a key event deepest-first through mounted `use_input` handlers,
    /// stopping early if a handler calls `stop_propagation`. Release events
    /// (Windows/kitty protocol) are dropped — only Press/Repeat are dispatched.
    pub fn dispatch_key(&mut self, ev: crossterm::event::KeyEvent) {
        if ev.kind == crossterm::event::KeyEventKind::Release {
            return;
        }
        let handlers = self.tree.collect_input_handlers();
        let mut ctx = crate::hooks::input::InputCtx { stopped: false };
        for h in handlers {
            h.borrow_mut()(ev, &mut ctx);
            if ctx.stopped {
                break;
            }
        }
    }

    fn depth(&self, mut id: FiberId) -> usize {
        let mut d = 0;
        while self.tree.contains(id) {
            match self.tree.get(id).parent {
                Some(p) => {
                    d += 1;
                    id = p;
                }
                None => break,
            }
        }
        d
    }

    /// Layout (if dirty), paint, diff against previous frame, flush.
    pub fn draw(&mut self, backend: &mut dyn Backend) -> Result<(), Error> {
        if self.tree.layout_dirty || self.prev.is_none() {
            compute_layout(&mut self.tree, self.size.0, self.size.1);
        }
        // Double-buffer: paint into `self.scratch` (the buffer from two
        // frames ago, if any) instead of allocating fresh every frame, diff
        // against `self.prev` (last frame's buffer), then swap the two
        // roles for next time. This avoids a Vec alloc+zero-fill per frame
        // in the common case where the terminal size hasn't changed.
        let mut buf = self
            .scratch
            .take()
            .unwrap_or_else(|| Buffer::new(self.size.0, self.size.1));
        buf.resize_and_clear(self.size.0, self.size.1);
        paint(&self.tree, &mut buf);
        let blank;
        let prev: &Buffer = match &self.prev {
            Some(p) => p,
            None => {
                blank = Buffer::new(self.size.0, self.size.1);
                &blank
            }
        };
        backend.flush(&buf.diff(prev))?;
        self.scratch = self.prev.take();
        self.prev = Some(buf);
        Ok(())
    }

    /// Inline mode: content-sized rows of the live region. Lays out at `width`
    /// (bounded to `max_height`), paints, and drops trailing blank rows.
    pub fn live_rows(&mut self, width: u16, max_height: u16) -> Vec<Vec<Cell>> {
        let size_changed = self.live_size != Some((width, max_height));
        if self.tree.layout_dirty || size_changed {
            compute_layout(&mut self.tree, width, max_height);
            self.live_size = Some((width, max_height));
        }
        let mut buf = Buffer::new(width, max_height);
        paint(&self.tree, &mut buf);
        trim_rows(buffer_rows(&buf))
    }

    /// Inline mode: drain queued scrollback elements, rendering each to rows to
    /// be committed permanently. Each element is laid out and painted on its own.
    pub fn take_committed(&mut self, width: u16, max_height: u16) -> Vec<Vec<Cell>> {
        let queued: Vec<Element> = std::mem::take(&mut *self.rt.scrollback.borrow_mut());
        let mut rows = Vec::new();
        for el in queued {
            let mut tree = FiberTree::new();
            tree.mount_root(el, &self.rt);
            compute_layout(&mut tree, width, max_height);
            let mut buf = Buffer::new(width, max_height);
            paint(&tree, &mut buf);
            rows.extend(trim_rows(buffer_rows(&buf)));
        }
        rows
    }

    // Used by the render() event-loop task to await the next wake.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub async fn wait_wake(&mut self) -> Option<Wake> {
        self.wake_rx.recv().await
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        self.size = (w, h);
        *self
            .rt
            .size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = (w, h);
        self.tree.layout_dirty = true;
        self.prev = None; // full repaint
        // Push the root as dirty so components reading use_terminal_size()
        // re-render with the new size. The caller must invoke
        // process_wakes() after resize() to actually apply this;
        // TestTerminal::resize does, and so does the render() loop, which
        // calls process_wakes() on its next iteration after Event::Resize.
        if let Some(root) = self.tree.root {
            self.pending.push(root);
        }
    }
}

/// Restores the terminal even when a panic unwinds through the render future.
pub(crate) struct RestoreGuard<'a, B: Backend + ?Sized> {
    pub backend: &'a mut B,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl<'a, B: Backend + ?Sized> Drop for RestoreGuard<'a, B> {
    fn drop(&mut self) {
        let _ = self.backend.leave();
    }
}

/// Belt-and-braces: if a panic is printed while the alternate screen is
/// active, the message would be invisible and raw mode would linger. Restore
/// first, then run the default hook.
///
/// Note: a panic inside a use_future/use_stream task fires this hook
/// (restoring the terminal) but does NOT stop the main loop — the app keeps
/// running against a restored screen. v1 accepts this seam; joining/
/// propagating task panics is future work.
#[cfg_attr(coverage_nightly, coverage(off))]
fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::cursor::Show,
                crossterm::terminal::LeaveAlternateScreen
            );
            prev(info);
        }));
    });
}

/// Run the app fullscreen until a component calls `use_app().exit()`.
/// Note: the returned future is !Send — use `#[tokio::main(flavor = "current_thread")]`.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn render(el: Element) -> Result<(), Error> {
    install_panic_hook();
    let mut backend = FullscreenBackend::new();
    backend.enter()?;
    let guard = RestoreGuard {
        backend: &mut backend,
    };
    run_loop(el, guard).await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn run_loop<B: Backend>(el: Element, guard: RestoreGuard<'_, B>) -> Result<(), Error> {
    let size = guard.backend.size()?;
    let mut core = AppCore::new(el, size);
    core.process_wakes();
    core.draw(guard.backend)?;

    let mut events = EventStream::new();
    let mut last_frame = Instant::now();

    while !core.exited {
        tokio::select! {
            ev = events.next() => match ev {
                Some(Ok(Event::Key(k))) => core.dispatch_key(k),
                Some(Ok(Event::Resize(w, h))) => core.resize(w, h),
                Some(Err(e)) => return Err(e.into()),
                None => break, // stdin closed
                _ => {}
            },
            w = core.wait_wake() => match w {
                Some(w) => core.apply_wake(w),
                None => break,
            },
        }
        // Drain any input burst (paste, key repeat) before the frame; bounded so wakes can't starve.
        for _ in 0..MAX_EVENT_BURST {
            match events.next().now_or_never() {
                Some(Some(Ok(Event::Key(k)))) => core.dispatch_key(k),
                Some(Some(Ok(Event::Resize(w, h)))) => core.resize(w, h),
                Some(Some(Ok(_))) => {}
                Some(Some(Err(e))) => return Err(e.into()),
                Some(None) | None => break,
            }
        }
        core.process_wakes();
        if core.exited {
            break;
        }
        // Coalesce: hold the frame if we're ahead of the cap, absorbing more wakes.
        let elapsed = last_frame.elapsed();
        if elapsed < FRAME {
            tokio::time::sleep(FRAME - elapsed).await;
            core.process_wakes();
            if core.exited {
                break;
            }
        }
        // Fullscreen never draws committed scrollback (only render_inline does),
        // so discard anything use_scrollback queued to keep it from growing without
        // bound. Committing under render() is a documented no-op.
        core.rt.scrollback.borrow_mut().clear();
        core.draw(guard.backend)?;
        last_frame = Instant::now();
    }
    Ok(()) // guard drops here -> leave()
}

/// Drop trailing all-blank rows so the live/committed region is content-sized.
fn trim_rows(mut rows: Vec<Vec<Cell>>) -> Vec<Vec<Cell>> {
    while rows
        .last()
        .is_some_and(|r| r.iter().all(|c| *c == Cell::default()))
    {
        rows.pop();
    }
    rows
}

/// Restores the terminal for inline mode even if a panic unwinds through it.
pub(crate) struct InlineRestoreGuard<'a, S: InlineSink + ?Sized> {
    pub backend: &'a mut S,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl<'a, S: InlineSink + ?Sized> Drop for InlineRestoreGuard<'a, S> {
    fn drop(&mut self) {
        let _ = self.backend.leave();
    }
}

/// Run an app **inline**: finished output committed via
/// [`use_scrollback`](crate::Hooks::use_scrollback) is printed permanently into
/// the terminal's real scrollback, while a live region at the bottom is redrawn
/// in place. Unlike [`render`], this does not use the alternate screen.
///
/// [`use_scrollback`] only draws under this entry point: committing under the
/// fullscreen [`render`] is a no-op (the queued content is discarded each frame).
///
/// The returned future is `!Send` — use `#[tokio::main(flavor = "current_thread")]`.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn render_inline(el: Element) -> Result<(), Error> {
    install_panic_hook();
    let mut backend = InlineBackend::new();
    backend.enter()?;
    let guard = InlineRestoreGuard {
        backend: &mut backend,
    };
    run_inline_loop(el, guard).await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn run_inline_loop<S: InlineSink>(
    el: Element,
    guard: InlineRestoreGuard<'_, S>,
) -> Result<(), Error> {
    let (w, h) = guard.backend.size()?;
    let mut core = AppCore::new(el, (w, h));
    core.process_wakes();
    commit_and_present(&mut core, guard.backend)?;

    let mut events = EventStream::new();
    let mut last_frame = Instant::now();
    while !core.exited {
        tokio::select! {
            ev = events.next() => match ev {
                Some(Ok(Event::Key(k))) => core.dispatch_key(k),
                Some(Ok(Event::Resize(nw, nh))) => core.resize(nw, nh),
                Some(Err(e)) => return Err(e.into()),
                None => break,
                _ => {}
            },
            wk = core.wait_wake() => match wk {
                Some(wk) => core.apply_wake(wk),
                None => break,
            },
        }
        for _ in 0..MAX_EVENT_BURST {
            match events.next().now_or_never() {
                Some(Some(Ok(Event::Key(k)))) => core.dispatch_key(k),
                Some(Some(Ok(Event::Resize(nw, nh)))) => core.resize(nw, nh),
                Some(Some(Ok(_))) => {}
                Some(Some(Err(e))) => return Err(e.into()),
                Some(None) | None => break,
            }
        }
        core.process_wakes();
        if core.exited {
            break;
        }
        let elapsed = last_frame.elapsed();
        if elapsed < FRAME {
            tokio::time::sleep(FRAME - elapsed).await;
            core.process_wakes();
            if core.exited {
                break;
            }
        }
        commit_and_present(&mut core, guard.backend)?;
        last_frame = Instant::now();
    }
    Ok(())
}

/// Commit any queued scrollback rows, then redraw the live region.
#[cfg_attr(coverage_nightly, coverage(off))]
fn commit_and_present<S: InlineSink>(core: &mut AppCore, backend: &mut S) -> Result<(), Error> {
    let (w, h) = core.size;
    let committed = core.take_committed(w, h);
    if !committed.is_empty() {
        backend.commit(&committed)?;
    }
    let live = core.live_rows(w, h);
    backend.present(&live)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TestBackend;
    use crate::backend::inline::{InlineSink, RecordingSink};
    use crate::component::Component;
    use crate::hooks::Hooks;
    use crate::props::{TextProps, ViewProps};

    #[test]
    fn restore_guard_leaves_on_drop() {
        let mut be = TestBackend::new(4, 2);
        be.enter().unwrap();
        {
            let _guard = RestoreGuard { backend: &mut be };
        }
        assert_eq!(be.lifecycle, vec!["enter", "leave"]);
    }

    struct Inline;
    impl Component for Inline {
        type Props = ();
        fn render(_: &(), hooks: &mut Hooks) -> Element {
            let sb = hooks.use_scrollback();
            hooks.use_effect((), move || {
                sb.commit(Element::text(TextProps {
                    content: "committed turn".into(),
                    ..Default::default()
                }));
            });
            Element::view(
                ViewProps::default(),
                vec![Element::text(TextProps {
                    content: "> live prompt".into(),
                    ..Default::default()
                })],
            )
        }
    }

    #[test]
    fn inline_splits_committed_scrollback_from_live_region() {
        let mut core = AppCore::new(Element::component::<Inline>(()), (20, 6));
        core.process_wakes(); // runs the mount effect, which queues a commit

        let mut sink = RecordingSink::new(20, 6);
        commit_and_present_into(&mut core, &mut sink);

        // Finished output went to scrollback; the prompt stayed in the live region.
        assert!(
            sink.committed.iter().any(|l| l.contains("committed turn")),
            "committed: {:?}",
            sink.committed
        );
        assert!(
            sink.live.iter().any(|l| l.contains("> live prompt")),
            "live: {:?}",
            sink.live
        );
        assert!(
            !sink.live.iter().any(|l| l.contains("committed turn")),
            "committed content must not remain in the live region"
        );
    }

    #[test]
    fn appcore_wakes_dispatch_release_and_resize_with_depth() {
        use crate::hooks::input::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        use crate::hooks::state::State;
        use crate::test_util::Shared;

        #[derive(Clone, PartialEq, Default)]
        struct CP {
            handle: Shared<Option<State<i32>>>,
            seen: Shared<i32>,
        }
        struct Child;
        impl Component for Child {
            type Props = CP;
            fn render(props: &CP, hooks: &mut Hooks) -> Element {
                let n = hooks.use_state(|| 0);
                *props.handle.lock() = Some(n.clone());
                *props.seen.lock() = n.get();
                Element::text(TextProps {
                    content: n.get().to_string(),
                    ..Default::default()
                })
            }
        }
        #[derive(Clone, PartialEq, Default)]
        struct PP {
            child: CP,
        }
        struct Parent;
        impl Component for Parent {
            type Props = PP;
            fn render(props: &PP, _hooks: &mut Hooks) -> Element {
                Element::view(
                    ViewProps::default(),
                    vec![Element::component::<Child>(props.child.clone())],
                )
            }
        }

        let props = PP::default();
        let mut core = AppCore::new(Element::component::<Parent>(props.clone()), (10, 5));
        core.process_wakes();

        // Dirty a non-root (child) fiber so depth() walks up to the root.
        let st = props.child.handle.lock().clone().unwrap();
        st.set(7);
        core.process_wakes();
        assert_eq!(*props.child.seen.lock(), 7);

        core.apply_wake(Wake::Redraw);
        core.resize(20, 8);
        core.process_wakes();
        assert_eq!(core.size, (20, 8));

        // A key Release is dropped (only Press/Repeat dispatch).
        core.dispatch_key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));

        core.apply_wake(Wake::Exit);
        assert!(core.exited);
    }

    #[test]
    fn recording_sink_lifecycle() {
        let mut s = crate::backend::inline::RecordingSink::new(4, 2);
        assert_eq!(s.size().unwrap(), (4, 2));
        s.enter().unwrap();
        s.leave().unwrap();
        assert_eq!(s.lifecycle, vec!["enter", "leave"]);
    }

    // Test shim mirroring commit_and_present for a concrete sink.
    fn commit_and_present_into<S: InlineSink>(core: &mut AppCore, sink: &mut S) {
        let (w, h) = core.size;
        let committed = core.take_committed(w, h);
        sink.commit(&committed).unwrap();
        let live = core.live_rows(w, h);
        sink.present(&live).unwrap();
    }

    #[test]
    fn depth_orders_dirty_fibers_shallowest_first() {
        use crate::hooks::state::State;
        use crate::test_util::Shared;

        // Root (depth 0) wraps Mid (deeper). Both log their id when they render;
        // dirtying both must re-render Root before Mid (shallowest-first).
        #[derive(Clone, PartialEq, Default)]
        struct Ctx {
            root_state: Shared<Option<State<i32>>>,
            mid_state: Shared<Option<State<i32>>>,
            log: Shared<Vec<char>>,
        }
        struct Mid;
        impl Component for Mid {
            type Props = Ctx;
            fn render(ctx: &Ctx, hooks: &mut Hooks) -> Element {
                let n = hooks.use_state(|| 0);
                *ctx.mid_state.lock() = Some(n.clone());
                ctx.log.lock().push('M');
                Element::text(TextProps {
                    content: n.get().to_string(),
                    ..Default::default()
                })
            }
        }
        struct Root;
        impl Component for Root {
            type Props = Ctx;
            fn render(ctx: &Ctx, hooks: &mut Hooks) -> Element {
                let n = hooks.use_state(|| 0);
                *ctx.root_state.lock() = Some(n.clone());
                ctx.log.lock().push('R');
                let _ = n.get();
                Element::view(
                    ViewProps::default(),
                    vec![Element::component::<Mid>(ctx.clone())],
                )
            }
        }

        let ctx = Ctx::default();
        let mut core = AppCore::new(Element::component::<Root>(ctx.clone()), (10, 5));
        core.process_wakes();
        ctx.log.lock().clear(); // drop the mount-time renders

        // Dirty both fibers in one batch so process_wakes must sort by depth.
        ctx.root_state.lock().clone().unwrap().set(1);
        ctx.mid_state.lock().clone().unwrap().set(1);
        core.process_wakes();

        // Root is shallower, so it re-renders first; Root's reconcile leaves Mid's
        // props unchanged (props_eq), so Mid re-renders exactly once, from its own
        // dirty mark — after Root.
        assert_eq!(*ctx.log.lock(), vec!['R', 'M']);
    }
}
