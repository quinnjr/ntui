//! Public test harness: drive an ntui app frame by frame without a terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::backend::TestBackend;
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
        t.core.draw(&mut t.backend)?;
        Ok(t)
    }

    /// Yield so hook-spawned tasks can run, then process wakes and redraw.
    pub async fn tick(&mut self) -> Result<(), Error> {
        for _ in 0..TASK_YIELD_BUDGET {
            tokio::task::yield_now().await;
        }
        self.core.process_wakes();
        self.core.draw(&mut self.backend)
    }

    /// Resizes the virtual terminal and redraws a full frame at the new
    /// size.
    pub fn resize(&mut self, width: u16, height: u16) -> Result<(), Error> {
        self.backend = TestBackend::new(width, height);
        self.core.resize(width, height);
        self.core.process_wakes();
        self.core.draw(&mut self.backend)
    }

    /// The current frame as a plain-text grid.
    pub fn frame_text(&self) -> String {
        self.backend.to_text()
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
        self.core.draw(&mut self.backend)
    }
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
}
