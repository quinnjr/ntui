use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::mpsc::UnboundedSender;

use crate::fiber::FiberId;
use crate::hooks::{Hooks, Wake};

#[derive(Default)]
struct ScrollInner {
    offset: u16,   // rows scrolled down from the top
    content: u16,  // total content height (set by layout each frame)
    viewport: u16, // visible height (set by layout each frame)
}

/// A scroll position for an [`Overflow::Scroll`](crate::Overflow::Scroll) `View`.
///
/// Obtain one with [`Hooks::use_scroll`] and pass a clone to the scrollable
/// `View` via `ViewProps::scroll`. Layout feeds the content and viewport
/// heights back into the handle each frame, so [`scroll_by`](Scroll::scroll_by)
/// and friends stay clamped, and the view **follows new content** (stays pinned
/// to the bottom) whenever it is already scrolled to the bottom — the behavior
/// a chat transcript wants.
#[derive(Clone)]
pub struct Scroll {
    inner: Arc<Mutex<ScrollInner>>,
    fiber: FiberId,
    wake: UnboundedSender<Wake>,
}

impl PartialEq for Scroll {
    /// Identity comparison — two handles are equal iff they share state.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for Scroll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.lock();
        f.debug_struct("Scroll")
            .field("offset", &g.offset)
            .field("content", &g.content)
            .field("viewport", &g.viewport)
            .finish()
    }
}

impl Scroll {
    pub(crate) fn new(fiber: FiberId, wake: UnboundedSender<Wake>) -> Self {
        Scroll {
            inner: Arc::new(Mutex::new(ScrollInner::default())),
            fiber,
            wake,
        }
    }

    fn lock(&self) -> MutexGuard<'_, ScrollInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Current scroll offset in rows from the top.
    pub fn offset(&self) -> u16 {
        self.lock().offset
    }

    /// Largest valid offset (`content - viewport`), i.e. fully scrolled down.
    pub fn max_offset(&self) -> u16 {
        let g = self.lock();
        g.content.saturating_sub(g.viewport)
    }

    /// Whether the view is scrolled to the bottom.
    pub fn at_bottom(&self) -> bool {
        let g = self.lock();
        g.offset >= g.content.saturating_sub(g.viewport)
    }

    /// Scroll by `delta` rows (negative = up), clamped to the valid range.
    pub fn scroll_by(&self, delta: i32) {
        {
            // Hold one lock across read-and-clamp: `set_metrics` (from layout,
            // possibly another thread) must not change the bounds mid-update.
            let mut g = self.lock();
            let max = g.content.saturating_sub(g.viewport) as i32;
            g.offset = (g.offset as i32 + delta).clamp(0, max) as u16;
        }
        self.wake();
    }

    /// Scroll to an absolute row offset, clamped to the valid range.
    pub fn scroll_to(&self, offset: u16) {
        {
            let mut g = self.lock();
            let max = g.content.saturating_sub(g.viewport);
            g.offset = offset.min(max);
        }
        self.wake();
    }

    /// Jump to the top.
    pub fn to_top(&self) {
        self.scroll_to(0);
    }

    /// Jump to the bottom.
    pub fn to_bottom(&self) {
        self.scroll_to(self.max_offset());
    }

    fn wake(&self) {
        let _ = self.wake.send(Wake::Dirty(self.fiber));
    }

    /// Called by layout each frame with freshly measured sizes. Re-clamps the
    /// offset, and keeps a bottom-pinned view pinned as content grows.
    pub(crate) fn set_metrics(&self, content: u16, viewport: u16) {
        let mut g = self.lock();
        let was_at_bottom = g.offset >= g.content.saturating_sub(g.viewport);
        g.content = content;
        g.viewport = viewport;
        let max = content.saturating_sub(viewport);
        g.offset = if was_at_bottom {
            max
        } else {
            g.offset.min(max)
        };
    }
}

impl<'a> Hooks<'a> {
    /// A persistent scroll position for an [`Overflow::Scroll`](crate::Overflow::Scroll)
    /// `View`. Pass a clone to the view's `ViewProps::scroll`; drive it from an
    /// input handler with [`Scroll::scroll_by`] / [`Scroll::to_bottom`].
    pub fn use_scroll(&mut self) -> Scroll {
        let fiber = self.fiber_id;
        let wake = self.runtime.wake.clone();
        self.use_state(move || Scroll::new(fiber, wake)).get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::fiber::FiberTree;
    use crate::hooks::RuntimeHandle;
    use crate::props::ViewProps;
    use crate::test_util::Shared;

    fn handle() -> Scroll {
        let (rt, rx) = RuntimeHandle::test_handle();
        std::mem::forget(rx); // keep the wake channel open
        Scroll::new(0, rt.wake.clone())
    }

    #[test]
    fn methods_clamp_debug_and_eq() {
        let s = handle();
        // Initial state is trivially "at bottom", so metrics pin to the bottom.
        s.set_metrics(10, 4);
        assert_eq!(s.max_offset(), 6);
        assert!(s.at_bottom());
        assert_eq!(s.offset(), 6);

        s.to_top();
        assert_eq!(s.offset(), 0);
        s.scroll_by(3);
        assert_eq!(s.offset(), 3);
        s.scroll_by(-1);
        assert_eq!(s.offset(), 2);
        s.scroll_by(100); // clamps to max
        assert_eq!(s.offset(), 6);
        s.scroll_to(1);
        assert_eq!(s.offset(), 1);
        s.to_bottom();
        assert_eq!(s.offset(), 6);

        // Not at bottom → set_metrics preserves (clamped) offset instead of pinning.
        s.scroll_to(2);
        s.set_metrics(20, 4);
        assert_eq!(s.offset(), 2);

        assert!(format!("{s:?}").contains("offset"));
        assert_eq!(s, s.clone());
        assert_ne!(s, handle());
    }

    #[test]
    fn use_scroll_returns_a_persistent_handle() {
        #[derive(Clone, PartialEq, Default)]
        struct P {
            out: Shared<Option<Scroll>>,
        }
        struct C;
        impl Component for C {
            type Props = P;
            fn render(props: &P, hooks: &mut Hooks) -> Element {
                *props.out.lock() = Some(hooks.use_scroll());
                Element::view(ViewProps::default(), vec![])
            }
        }
        let (rt, rx) = RuntimeHandle::test_handle();
        std::mem::forget(rx);
        let mut tree = FiberTree::new();
        let props = P::default();
        let root = tree.mount_root(Element::component::<C>(props.clone()), &rt);
        let first = props.out.lock().clone().unwrap();
        tree.render_fiber(root, &rt);
        let second = props.out.lock().clone().unwrap();
        assert_eq!(first, second, "same handle persists across renders");
    }
}
