use std::collections::HashSet;

use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::backend::Backend;
use crate::buffer::Buffer;
use crate::element::Element;
use crate::error::Error;
use crate::fiber::{FiberId, FiberTree};
use crate::hooks::{RuntimeHandle, Wake};
use crate::layout::compute_layout;
use crate::paint::paint;

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

    // Used by the render()/event-loop task (not yet implemented) to await the next wake.
    #[allow(dead_code)]
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
