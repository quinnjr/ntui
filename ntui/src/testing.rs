//! Public test harness: drive an ntui app frame by frame without a terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::backend::{Backend, TestBackend};
use crate::buffer::{Buffer, Cell};
use crate::element::Element;
use crate::error::Error;
use crate::runtime::AppCore;

/// Budget of cooperative yields so hook-spawned tokio tasks (later tasks) get
/// scheduled on the current-thread runtime before we drain wakes. Revisit if
/// async chains need more hops.
const TASK_YIELD_BUDGET: usize = 8;

/// A headless terminal that drives an ntui component tree frame by frame,
/// for use in tests. Mirrors what [`crate::render`] does against a real
/// terminal, but under manual control and without crossterm I/O.
pub struct TestTerminal {
    core: AppCore,
    backend: TestBackend,
}

impl TestTerminal {
    /// Mounts `el` at `width` x `height`, processes any mount-time wakes,
    /// and renders the first frame.
    pub fn new(width: u16, height: u16, el: Element) -> Result<Self, Error> {
        let mut t = TestTerminal {
            core: AppCore::new(el, (width, height)),
            backend: TestBackend::new(width, height),
        };
        t.core.process_wakes();
        t.flush_clipboard();
        t.core.draw(&mut t.backend)?;
        Ok(t)
    }

    /// Yield so hook-spawned tasks can run, then process wakes and redraw.
    pub async fn tick(&mut self) -> Result<(), Error> {
        for _ in 0..TASK_YIELD_BUDGET {
            tokio::task::yield_now().await;
        }
        self.core.process_wakes();
        self.flush_clipboard();
        self.core.draw(&mut self.backend)
    }

    /// Resizes the virtual terminal and redraws a full frame at the new
    /// size. Clipboard history is kept across the resize.
    pub fn resize(&mut self, width: u16, height: u16) -> Result<(), Error> {
        let clipboard = std::mem::take(&mut self.backend.clipboard);
        self.backend = TestBackend::new(width, height);
        self.backend.clipboard = clipboard;
        self.core.resize(width, height);
        self.core.process_wakes();
        self.flush_clipboard();
        self.core.draw(&mut self.backend)
    }

    /// The current frame as a plain-text grid.
    ///
    /// Styling is not represented. To assert on color, weight, or
    /// background — a selected row's highlight, a marked column header —
    /// use [`TestTerminal::cell`] or [`TestTerminal::buffer`].
    pub fn frame_text(&self) -> String {
        self.backend.to_text()
    }

    /// The painted cell at `x`, `y`, with its colors and attributes.
    ///
    /// `None` when the coordinates are outside the terminal.
    ///
    /// ```
    /// # use ntui::{element, Color, testing::TestTerminal};
    /// let t = TestTerminal::new(5, 1, element!(Text(content: "hi", color: Color::Red))).unwrap();
    /// assert_eq!(t.cell(0, 0).unwrap().ch, 'h');
    /// assert_eq!(t.cell(0, 0).unwrap().fg, Color::Red);
    /// ```
    pub fn cell(&self, x: u16, y: u16) -> Option<Cell> {
        let buffer = self.buffer();
        (x < buffer.width() && y < buffer.height()).then(|| *buffer.get(x, y))
    }

    /// The whole painted frame, styling included.
    ///
    /// [`frame_text`](TestTerminal::frame_text) is the readable view of the
    /// same data; this is the one to reach for when the property under test
    /// is visual rather than textual.
    pub fn buffer(&self) -> &Buffer {
        &self.backend.buffer
    }

    /// The cells of row `y`, left to right. Empty if `y` is off-screen.
    ///
    /// Convenient for asserting that a whole row shares a background — the
    /// usual shape of a selection highlight.
    pub fn row(&self, y: u16) -> Vec<Cell> {
        let buffer = self.buffer();
        if y >= buffer.height() {
            return Vec::new();
        }
        (0..buffer.width()).map(|x| *buffer.get(x, y)).collect()
    }

    /// Whether the app has called `use_app().exit()`.
    pub fn exited(&self) -> bool {
        self.core.exited
    }

    /// Sends a key press with no modifiers and redraws.
    pub fn send_key(&mut self, code: KeyCode) -> Result<(), Error> {
        self.send_key_event(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// Dispatches a key event through mounted `use_input` handlers, applies
    /// any resulting wakes, and redraws.
    pub fn send_key_event(&mut self, ev: KeyEvent) -> Result<(), Error> {
        self.core.dispatch_key(ev);
        self.core.process_wakes();
        self.flush_clipboard();
        self.core.draw(&mut self.backend)
    }

    /// Dispatches a bracketed-paste event through mounted `use_paste`
    /// handlers, applies any resulting wakes, and redraws.
    pub fn send_paste(&mut self, text: &str) -> Result<(), Error> {
        self.core.dispatch_paste(text);
        self.core.process_wakes();
        self.flush_clipboard();
        self.core.draw(&mut self.backend)
    }

    /// Clipboard payloads the app has sent via `AppHandle::copy_to_clipboard`
    /// and that have been flushed to the (test) backend, oldest first.
    pub fn clipboard(&self) -> &[String] {
        &self.backend.clipboard
    }

    fn flush_clipboard(&mut self) {
        for text in self.core.take_pending_clipboard() {
            self.backend.copy_to_clipboard(&text).unwrap();
        }
    }
}

/// Render one component once, outside any app, and return its element tree.
///
/// [`TestTerminal`] answers "what did the user see"; this answers "what did
/// this component build". Those differ in ways that matter: a frame cannot
/// distinguish an element that was never built from one that was built and
/// then clipped, so a component that deliberately skips offscreen work can
/// only be checked by counting what it produced.
///
/// ```
/// use ntui::{Component, Element, Hooks, props::TextProps, testing::render_once};
///
/// struct Greeting;
/// #[derive(Clone, PartialEq, Default)]
/// struct GreetingProps { name: String }
/// impl Component for Greeting {
///     type Props = GreetingProps;
///     fn render(props: &GreetingProps, _: &mut Hooks) -> Element {
///         Element::text(TextProps { content: format!("hi {}", props.name), ..Default::default() })
///     }
/// }
///
/// let el = render_once::<Greeting>(&GreetingProps { name: "ada".into() });
/// let ntui::Node::Text { props } = el.node else { panic!("expected text") };
/// assert_eq!(props.content, "hi ada");
/// ```
///
/// The `Hooks` handed to the component is detached from any fiber tree,
/// and there is no commit phase:
///
/// - `use_state` works, but setting it wakes nothing — there is no runtime
///   to re-render into, and the returned element is the single render.
/// - **`use_effect` bodies never run.** Effects are run by
///   `flush_effects` after commit, which this does not perform, so an
///   effect is registered and then dropped without firing — and its
///   cleanup never runs either, because it never produced one.
/// - By extension `use_task` and `use_interval`, which are built on
///   `use_effect`, spawn nothing here. They are silent no-ops rather than
///   a runtime requirement.
/// - `use_future` is the exception: it spawns during the render itself, so
///   it *does* need a tokio runtime, and panics outside one.
/// - `use_context` sees no providers, since there is no tree above.
///
/// This renders exactly once, so it cannot exercise anything that depends
/// on a second pass or on effects — reach for [`TestTerminal`] there.
pub fn render_once<C: crate::component::Component>(props: &C::Props) -> Element {
    /// Fiber id for a render with no tree behind it.
    ///
    /// Ids are allocated upward from 0, so this cannot collide with a real
    /// fiber. The wake channel is dead anyway, but relying on that alone
    /// would make the isolation an accident of plumbing rather than a
    /// stated property.
    const DETACHED_FIBER: crate::fiber::FiberId = crate::fiber::FiberId::MAX;

    let mut slots = Vec::new();
    // The receiver is dropped immediately: `State::set` and friends ignore a
    // closed channel, which is exactly the "wakes nothing" behavior wanted.
    let (wake, _) = tokio::sync::mpsc::unbounded_channel();
    let runtime = crate::hooks::RuntimeHandle {
        wake,
        size: std::sync::Arc::new(std::sync::Mutex::new((80, 24))),
        scrollback: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
    };

    let element = {
        let mut hooks = crate::hooks::Hooks::new(
            &mut slots,
            std::any::type_name::<C>(),
            DETACHED_FIBER,
            runtime,
            true,
            std::rc::Rc::new(crate::fiber::ContextMap::default()),
        );
        C::render(props, &mut hooks)
    };

    // Teardown, so anything `use_future` spawned during the render does not
    // outlive the call. Effect slots have no cleanup to run — their bodies
    // never fired.
    for slot in slots {
        slot.unmount();
    }

    element
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::Hooks;
    use crate::props::TextProps;

    struct Boot;
    #[derive(Clone, PartialEq, Default)]
    struct BootProps;
    impl Component for Boot {
        type Props = BootProps;
        fn render(_: &BootProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 0);
            let n2 = n.clone();
            hooks.use_effect((), move || n2.set(1)); // mount effect schedules an update
            Element::text(TextProps {
                content: format!("n={}", n.get()),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn mount_effects_and_wakes_are_processed_before_first_frame() {
        let t = TestTerminal::new(10, 1, Element::component::<Boot>(BootProps)).unwrap();
        assert!(t.frame_text().contains("n=1"));
    }

    struct NeverConverges;
    #[derive(Clone, PartialEq, Default)]
    struct NeverConvergesProps;
    impl Component for NeverConverges {
        type Props = NeverConvergesProps;
        fn render(_: &NeverConvergesProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 0);
            let n2 = n.clone();
            // deps change every pass, so this effect fires again every pass,
            // which dirties state again every pass: a non-converging fixpoint.
            hooks.use_effect(n.get(), move || n2.update(|v| *v += 1));
            Element::text(TextProps {
                content: format!("n={}", n.get()),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    #[should_panic(expected = "maximum update depth")]
    async fn process_wakes_panics_on_non_converging_fixpoint() {
        let _ = TestTerminal::new(
            10,
            1,
            Element::component::<NeverConverges>(NeverConvergesProps),
        );
    }

    // ---------- styled cell access ----------

    use crate::element::Node;
    use crate::props::ViewProps;
    use crate::style::{Color, Weight};

    #[tokio::test]
    async fn cell_reports_colors_that_frame_text_cannot() {
        let el = Element::view(
            ViewProps {
                background: Color::Blue,
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: "hi".into(),
                color: Color::Red,
                weight: Weight::Bold,
                ..Default::default()
            })],
        );
        let t = TestTerminal::new(4, 1, el).unwrap();

        let cell = t.cell(0, 0).expect("in bounds");
        assert_eq!(cell.ch, 'h');
        assert_eq!(cell.fg, Color::Red);
        assert_eq!(cell.bg, Color::Blue);
        assert!(cell.attrs.bold);
    }

    #[tokio::test]
    async fn cell_is_none_outside_the_terminal() {
        let t = TestTerminal::new(2, 1, Element::fragment(vec![])).unwrap();

        assert!(t.cell(2, 0).is_none());
        assert!(t.cell(0, 1).is_none());
    }

    #[tokio::test]
    async fn row_returns_the_whole_line_and_nothing_off_screen() {
        let el = Element::view(
            ViewProps {
                background: Color::Green,
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: "ab".into(),
                ..Default::default()
            })],
        );
        let t = TestTerminal::new(3, 1, el).unwrap();

        let row = t.row(0);
        assert_eq!(row.len(), 3, "one cell per column, even past the content");
        // The view is content-sized, so it paints its two cells and no more.
        assert_eq!(row[0].bg, Color::Green);
        assert_eq!(row[1].bg, Color::Green);
        assert_eq!(row[2].bg, Color::Reset, "beyond the box, nothing painted");
        assert!(t.row(9).is_empty(), "off-screen rows are empty");
    }

    // ---------- render_once ----------

    struct Counted;
    #[derive(Clone, PartialEq, Default)]
    struct CountedProps {
        total: usize,
        visible: usize,
    }
    impl Component for Counted {
        type Props = CountedProps;
        fn render(props: &CountedProps, _: &mut Hooks) -> Element {
            // Deliberately builds only the visible slice — the property a
            // rendered frame cannot distinguish from clipping.
            Element::view(
                ViewProps::default(),
                (0..props.visible.min(props.total))
                    .map(|i| {
                        Element::text(TextProps {
                            content: format!("row {i}"),
                            ..Default::default()
                        })
                    })
                    .collect(),
            )
        }
    }

    #[test]
    fn render_once_exposes_what_the_component_built() {
        let el = render_once::<Counted>(&CountedProps {
            total: 500,
            visible: 3,
        });

        let Node::View { children, .. } = el.node else {
            panic!("expected a view")
        };
        assert_eq!(children.len(), 3, "offscreen rows were never built");
    }

    #[test]
    fn render_once_needs_no_runtime_for_a_hook_free_component() {
        // Deliberately not a #[tokio::test]: a component that touches no
        // async hook must be renderable from a plain test.
        let el = render_once::<Counted>(&CountedProps {
            total: 0,
            visible: 0,
        });

        let Node::View { children, .. } = el.node else {
            panic!("expected a view")
        };
        assert!(children.is_empty());
    }

    struct Stateful;
    #[derive(Clone, PartialEq, Default)]
    struct StatefulProps;
    impl Component for Stateful {
        type Props = StatefulProps;
        fn render(_: &StatefulProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 7i32);
            // Setting must not panic even though nothing is listening.
            n.set(n.get() + 1);
            Element::text(TextProps {
                content: format!("n={}", n.get()),
                ..Default::default()
            })
        }
    }

    struct WithEffect;
    #[derive(Clone, PartialEq, Default)]
    struct WithEffectProps {
        ran: crate::test_util::Shared<bool>,
    }
    impl Component for WithEffect {
        type Props = WithEffectProps;
        fn render(props: &WithEffectProps, hooks: &mut Hooks) -> Element {
            let ran = props.ran.clone();
            hooks.use_effect((), move || *ran.lock() = true);
            Element::text(TextProps::default())
        }
    }

    #[test]
    fn render_once_does_not_run_effects() {
        // There is no commit phase, so `flush_effects` never runs. Pinned
        // because the doc makes the claim and a caller reaching for
        // `use_task` here would otherwise see a silent no-op.
        let ran = crate::test_util::Shared::default();

        render_once::<WithEffect>(&WithEffectProps { ran: ran.clone() });

        assert!(!*ran.lock(), "effect bodies must not fire");
    }

    #[test]
    fn render_once_tolerates_state_hooks_with_no_runtime_behind_them() {
        let el = render_once::<Stateful>(&StatefulProps);

        let Node::Text { props } = el.node else {
            panic!("expected text")
        };
        assert_eq!(props.content, "n=8");
    }
}
