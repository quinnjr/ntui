use std::time::Duration;

use tokio::time::Instant;

use crate::hooks::Hooks;

impl<'a> Hooks<'a> {
    /// Calls `on_tick` roughly every `period`, for as long as the component
    /// stays mounted, starting after the first `period` elapses (not
    /// immediately on mount). Spawns one task on mount, like [`Hooks::use_future`];
    /// `on_tick` typically closes over a [`crate::State`] handle to drive an
    /// animation or a periodic refresh.
    ///
    /// If `on_tick` panics, the underlying task is silently aborted and the
    /// interval stops ticking permanently for the rest of the component's
    /// lifetime, with no error surfaced — same caveat as [`Hooks::use_future`].
    pub fn use_interval(&mut self, period: Duration, mut on_tick: impl FnMut() + Send + 'static) {
        self.use_future(move || async move {
            let mut interval = tokio::time::interval(period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await; // first tick fires immediately; skip it.
            loop {
                interval.tick().await;
                on_tick();
            }
        });
    }
}

#[derive(Clone, Copy)]
struct TweenState {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

fn value_at(s: &TweenState, now: Instant) -> f32 {
    if s.duration.is_zero() {
        return s.to;
    }
    let t = (now.saturating_duration_since(s.start).as_secs_f32() / s.duration.as_secs_f32())
        .clamp(0.0, 1.0);
    // ease-out-cubic: fast start, settling smoothly into the target.
    let eased = 1.0 - (1.0 - t).powi(3);
    s.from + (s.to - s.from) * eased
}

impl<'a> Hooks<'a> {
    /// Animates toward `target` over `duration`, returning the current
    /// interpolated value on every render. Retargeting (`target` changing
    /// between renders) restarts the animation from wherever it currently
    /// is, rather than jumping.
    ///
    /// Composite hook: internally one [`Hooks::use_state`] holding the
    /// animation's endpoints, one [`Hooks::use_effect`] that retargets it,
    /// and one [`Hooks::use_task`] driving a ~60Hz re-render tick that runs
    /// only while the animation is in flight — the task exits once the tween
    /// settles and is respawned by the next retarget, so an idle animated
    /// widget holds no live timer.
    ///
    /// If the internal driving task ever panics, it is silently aborted and
    /// the current animation stops advancing, with no error surfaced — same
    /// caveat as [`Hooks::use_future`] (the next retarget spawns a fresh
    /// driver, so later animations still run).
    pub fn use_tween(&mut self, target: f32, duration: Duration) -> f32 {
        let state = self.use_state(|| TweenState {
            from: target,
            to: target,
            start: Instant::now(),
            duration,
        });

        let retarget = state.clone();
        self.use_effect(target.to_bits(), move || {
            let now = Instant::now();
            retarget.update(|s| {
                let current = value_at(s, now);
                *s = TweenState {
                    from: current,
                    to: target,
                    start: now,
                    duration,
                };
            });
        });

        // Declared after the retarget effect above so that, sharing the same
        // deps, the endpoints are updated before each fresh driver spawns.
        let tick = state.clone();
        self.use_task(target.to_bits(), move || async move {
            loop {
                tokio::time::sleep(Duration::from_millis(16)).await;
                let s = tick.get();
                let done = s.from == s.to
                    || Instant::now().saturating_duration_since(s.start) >= s.duration;
                // Value is recomputed at read-time from (from, to, start);
                // this update only exists to force the re-render. The final
                // update lands one frame at (or past) the endpoint before
                // the task exits.
                tick.update(|_| {});
                if done {
                    break;
                }
            }
        });

        value_at(&state.get(), Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::state::State;
    use crate::props::TextProps;
    use crate::test_util::Shared;
    use crate::testing::TestTerminal;

    struct Ticker;
    #[derive(Clone, PartialEq, Default)]
    struct TickerProps {
        count: Shared<Option<State<u32>>>,
    }
    impl Component for Ticker {
        type Props = TickerProps;
        fn render(props: &TickerProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 0u32);
            *props.count.lock() = Some(n.clone());
            let inc = n.clone();
            hooks.use_interval(Duration::from_millis(100), move || {
                inc.update(|v| *v += 1);
            });
            Element::text(TextProps {
                content: n.get().to_string(),
                ..Default::default()
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn interval_does_not_fire_before_the_first_period() {
        let props = TickerProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Ticker>(props.clone())).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        t.tick().await.unwrap();
        assert!(t.frame_text().contains('0'));
    }

    #[tokio::test(start_paused = true)]
    async fn interval_fires_repeatedly() {
        let props = TickerProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Ticker>(props.clone())).unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        t.tick().await.unwrap();
        assert_eq!(props.count.lock().clone().unwrap().get(), 3);
    }

    struct Tween;
    #[derive(Clone, PartialEq, Default)]
    struct TweenProps {
        target: Shared<Option<State<f32>>>,
        last: Shared<f32>,
    }
    impl Component for Tween {
        type Props = TweenProps;
        fn render(props: &TweenProps, hooks: &mut Hooks) -> Element {
            let target = hooks.use_state(|| 0.0f32);
            *props.target.lock() = Some(target.clone());
            let v = hooks.use_tween(target.get(), Duration::from_millis(200));
            *props.last.lock() = v;
            Element::text(TextProps {
                content: format!("{v:.2}"),
                ..Default::default()
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn tween_starts_at_the_initial_target() {
        let props = TweenProps::default();
        let t = TestTerminal::new(10, 1, Element::component::<Tween>(props.clone())).unwrap();
        assert!(t.frame_text().contains("0.00"));
    }

    #[tokio::test(start_paused = true)]
    async fn tween_moves_toward_a_new_target_and_settles() {
        let props = TweenProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Tween>(props.clone())).unwrap();
        props.target.lock().clone().unwrap().set(10.0);
        t.tick().await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        t.tick().await.unwrap();
        let mid = *props.last.lock();
        assert!(mid > 0.0 && mid < 10.0, "should be partway there: {mid}");

        tokio::time::sleep(Duration::from_millis(300)).await;
        t.tick().await.unwrap();
        assert!(t.frame_text().contains("10.00"), "should settle at target");
    }

    #[tokio::test(start_paused = true)]
    async fn retargeting_mid_animation_continues_smoothly_instead_of_jumping() {
        let props = TweenProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Tween>(props.clone())).unwrap();
        props.target.lock().clone().unwrap().set(10.0);
        t.tick().await.unwrap();

        // Advance partway through the 200ms tween toward 10.0.
        tokio::time::sleep(Duration::from_millis(100)).await;
        t.tick().await.unwrap();
        let v1 = *props.last.lock();
        assert!(v1 > 0.0 && v1 < 10.0, "should be partway there: {v1}");

        // Retarget immediately, without letting any more time pass.
        props.target.lock().clone().unwrap().set(20.0);
        t.tick().await.unwrap();
        let v2 = *props.last.lock();

        // The value right after retargeting should continue from v1, not
        // jump back to the original start (0.0) or snap to the old target (10.0).
        assert!(
            (v2 - v1).abs() < 0.5,
            "retarget should continue smoothly from {v1}, got {v2}"
        );
        assert!(
            v2 > 1.0,
            "should not have jumped back to the old start value: {v2}"
        );
    }
}
