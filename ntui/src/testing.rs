//! Public test harness: drive an ntui app frame by frame without a terminal.

use crate::backend::TestBackend;
use crate::element::Element;
use crate::error::Error;
use crate::runtime::AppCore;

pub struct TestTerminal {
    core: AppCore,
    backend: TestBackend,
}

impl TestTerminal {
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
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        self.core.process_wakes();
        self.core.draw(&mut self.backend)
    }

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

    pub fn exited(&self) -> bool {
        self.core.exited
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
}
