use std::collections::HashSet;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::backend::{Backend, FullscreenBackend};
use crate::buffer::Buffer;
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
    pub size: (u16, u16),
    pub exited: bool,
}

impl AppCore {
    pub fn new(el: Element, size: (u16, u16)) -> Self {
        let (wake, wake_rx) = unbounded_channel();
        let rt = RuntimeHandle {
            wake,
            size: std::sync::Arc::new(std::sync::Mutex::new(size)),
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
            size,
            exited: false,
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
        let mut buf = Buffer::new(self.size.0, self.size.1);
        paint(&self.tree, &mut buf);
        let blank;
        let prev = match &self.prev {
            Some(p) => p,
            None => {
                blank = Buffer::new(self.size.0, self.size.1);
                &blank
            }
        };
        backend.flush(&buf.diff(prev))?;
        self.prev = Some(buf);
        Ok(())
    }

    // Used by the render() event-loop task to await the next wake.
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
        // TestTerminal::resize does, and the future render loop must too.
        if let Some(root) = self.tree.root {
            self.pending.push(root);
        }
    }
}

/// Restores the terminal even when a panic unwinds through the render future.
pub(crate) struct RestoreGuard<'a, B: Backend + ?Sized> {
    pub backend: &'a mut B,
}

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
pub async fn render(el: Element) -> Result<(), Error> {
    install_panic_hook();
    let mut backend = FullscreenBackend::new();
    backend.enter()?;
    let guard = RestoreGuard {
        backend: &mut backend,
    };
    run_loop(el, guard).await
}

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
        core.draw(guard.backend)?;
        last_frame = Instant::now();
    }
    Ok(()) // guard drops here -> leave()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TestBackend;

    #[test]
    fn restore_guard_leaves_on_drop() {
        let mut be = TestBackend::new(4, 2);
        be.enter().unwrap();
        {
            let _guard = RestoreGuard { backend: &mut be };
        }
        assert_eq!(be.lifecycle, vec!["enter", "leave"]);
    }
}
