use crate::hooks::{Hooks, Wake};

/// Handle to the running app; clonable into closures and tasks.
#[derive(Clone)]
pub struct AppHandle {
    pub(crate) wake: tokio::sync::mpsc::UnboundedSender<Wake>,
}

impl AppHandle {
    /// Ask the event loop to shut down after the current pass.
    pub fn exit(&self) {
        let _ = self.wake.send(Wake::Exit);
    }

    /// Force a full re-render from the root (rarely needed; state changes
    /// already schedule renders).
    pub fn redraw(&self) {
        let _ = self.wake.send(Wake::Redraw);
    }
}

impl<'a> Hooks<'a> {
    /// Not slot-based; safe to call conditionally. `use_app` just clones a
    /// channel handle off `self.runtime` — it doesn't allocate a hook slot,
    /// so call order/count doesn't matter and it can be called behind
    /// conditionals without breaking hook identity for other hooks.
    pub fn use_app(&mut self) -> AppHandle {
        AppHandle {
            wake: self.runtime.wake.clone(),
        }
    }

    /// Current terminal size. Components re-render on resize (the root is
    /// marked dirty), so reads stay fresh. Not slot-based, for the same
    /// reason as `use_app`: it only reads a shared `Arc<Mutex<_>>`, no
    /// per-fiber storage is involved.
    pub fn use_terminal_size(&mut self) -> (u16, u16) {
        *self
            .runtime
            .size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::Hooks;
    use crate::hooks::input::KeyCode;
    use crate::props::TextProps;
    use crate::testing::TestTerminal;

    struct App;
    #[derive(Clone, PartialEq, Default)]
    struct AppProps;
    impl Component for App {
        type Props = AppProps;
        fn render(_: &AppProps, hooks: &mut Hooks) -> Element {
            let (w, h) = hooks.use_terminal_size();
            let app = hooks.use_app();
            hooks.use_input(move |ev, _| {
                if ev.code == KeyCode::Char('q') {
                    app.exit();
                }
            });
            Element::text(TextProps {
                content: format!("{w}x{h}"),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn terminal_size_updates_on_resize_and_exit_works() {
        let mut t = TestTerminal::new(10, 2, Element::component::<App>(AppProps)).unwrap();
        assert!(t.frame_text().contains("10x2"));
        t.resize(20, 5).unwrap();
        assert!(t.frame_text().contains("20x5"));
        assert!(!t.exited());
        t.send_key(KeyCode::Char('q')).unwrap();
        assert!(t.exited());
    }

    #[test]
    fn app_handle_redraw_and_exit_send_wakes() {
        use crate::component::Component;
        use crate::element::Element;
        use crate::fiber::FiberTree;
        use crate::hooks::{Hooks, RuntimeHandle, Wake};
        use crate::props::ViewProps;
        use crate::test_util::Shared;

        #[derive(Clone, PartialEq, Default)]
        struct P {
            h: Shared<Option<super::AppHandle>>,
        }
        struct C;
        impl Component for C {
            type Props = P;
            fn render(props: &P, hooks: &mut Hooks) -> Element {
                *props.h.lock() = Some(hooks.use_app());
                Element::view(ViewProps::default(), vec![])
            }
        }
        let (rt, mut rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let props = P::default();
        tree.mount_root(Element::component::<C>(props.clone()), &rt);
        let app = props.h.lock().clone().unwrap();
        while rx.try_recv().is_ok() {} // drain mount wakes
        app.redraw();
        assert!(matches!(rx.try_recv(), Ok(Wake::Redraw)));
        app.exit();
        assert!(matches!(rx.try_recv(), Ok(Wake::Exit)));
    }
}
