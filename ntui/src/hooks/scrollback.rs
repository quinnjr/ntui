use std::cell::RefCell;
use std::rc::Rc;

use crate::element::Element;
use crate::fiber::FiberId;
use crate::hooks::{Hooks, Wake};

/// Commits finished output to the terminal's real scrollback in inline mode
/// (see [`render_inline`](crate::render_inline)).
///
/// Content passed to [`commit`](Scrollback::commit) is printed **permanently**
/// above the live region and scrolls into the terminal's history — the user can
/// scroll it back with the mouse/terminal like any other output. The live
/// region (input, spinner, in-progress output) stays at the bottom and is
/// redrawn in place.
///
/// Committed elements should be static (plain `View`/`Text`, no hooks/state):
/// they are rendered once. The typical pattern is a chat that `commit`s each
/// finished turn and drops it from its live state.
///
/// Outside inline mode (i.e. under [`render`](crate::render)) commits are
/// queued but never drawn.
#[derive(Clone)]
pub struct Scrollback {
    queue: Rc<RefCell<Vec<Element>>>,
    fiber: FiberId,
    wake: tokio::sync::mpsc::UnboundedSender<Wake>,
}

impl Scrollback {
    /// Print `element` permanently to the terminal's scrollback on the next frame.
    pub fn commit(&self, element: Element) {
        self.queue.borrow_mut().push(element);
        let _ = self.wake.send(Wake::Dirty(self.fiber));
    }
}

impl<'a> Hooks<'a> {
    /// A [`Scrollback`] handle for committing finished output to terminal
    /// scrollback in inline mode.
    pub fn use_scrollback(&mut self) -> Scrollback {
        Scrollback {
            queue: self.runtime.scrollback.clone(),
            fiber: self.fiber_id,
            wake: self.runtime.wake.clone(),
        }
    }
}
