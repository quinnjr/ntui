//! A single-screen tour of every widget in `ntui::widgets`, all sharing one
//! focus scope: Tab / Shift-Tab moves between them, Enter/Space activates
//! the focused one, and Left/Right/Up/Down drive whichever widget currently
//! holds focus (Select, Tabs).
//!
//! Run: `cargo run --example widgets_gallery`
//! Keys: Tab / Shift-Tab to move focus · q to quit.

use ntui::widgets::{
    Badge, BadgeProps, Banner, BannerProps, Button, ButtonProps, Callback, Checkbox, CheckboxProps,
    Divider, DividerProps, GradientText, GradientTextProps, Modal, ModalProps, ProgressBar,
    ProgressBarProps, Select, SelectProps, Spinner, SpinnerProps, Table, TableProps, Tabs,
    TabsProps, TextInput, TextInputProps, Toast, ToastProps, Toggle, ToggleProps, Tone, Tooltip,
    TooltipProps,
};
use ntui::{BorderStyle, Color, FlexDirection, KeyCode, component, element, render};

#[component]
fn App(hooks: &mut ntui::Hooks) -> ntui::Element {
    let app = hooks.use_app();
    let scope = hooks.use_focus_scope();

    let name = hooks.use_state(String::new);
    let agree = hooks.use_state(|| false);
    let notifications = hooks.use_state(|| true);
    let tab = hooks.use_state(|| 0usize);
    let choice = hooks.use_state(|| 0usize);
    let presses = hooks.use_state(|| 0u32);
    // Progress climbs a little with each Submit press, just so the bar (and
    // its tween) has something to visibly animate toward.
    let progress = hooks.use_state(|| 0.15f32);
    let show_modal = hooks.use_state(|| false);

    hooks.use_input(move |ev, _| {
        if ev.code == KeyCode::Char('q') {
            app.exit();
        }
    });

    let (n, a, no, t, ch) = (
        name.clone(),
        agree.clone(),
        notifications.clone(),
        tab.clone(),
        choice.clone(),
    );
    let p = presses.clone();
    let pr = progress.clone();
    let sm_open = show_modal.clone();
    let sm_close = show_modal.clone();

    element! {
        ContextProvider(value: scope) {
            View(flex_direction: FlexDirection::Column, gap: 1, padding: 1) {
                Banner(title: "ntui::widgets".to_string(), subtitle: "q to quit · Tab / Shift-Tab to move focus".to_string())

                Tabs(
                    labels: vec!["Form".to_string(), "Data".to_string()],
                    active: tab.get(),
                    on_change: Some(Callback::new(move |i| t.set(i)))
                )

                #(if tab.get() == 0 {
                    Some(element! {
                        View(flex_direction: FlexDirection::Column, gap: 1, border_style: BorderStyle::Round, padding: 1) {
                            TextInput(
                                value: name.get(),
                                placeholder: "your name".to_string(),
                                on_change: Some(Callback::new(move |s| n.set(s)))
                            )
                            Checkbox(
                                label: "I agree to the terms".to_string(),
                                checked: agree.get(),
                                on_change: Some(Callback::new(move |v| a.set(v)))
                            )
                            Toggle(
                                label: "notifications".to_string(),
                                on: notifications.get(),
                                on_change: Some(Callback::new(move |v| no.set(v)))
                            )
                            Select(
                                items: vec!["Small".to_string(), "Medium".to_string(), "Large".to_string()],
                                selected: choice.get(),
                                on_change: Some(Callback::new(move |i| ch.set(i)))
                            )
                            Button(
                                label: format!("Submit ({})", presses.get()),
                                on_press: Some(Callback::new(move |()| {
                                    p.update(|n| *n += 1);
                                    pr.update(|v| *v = (*v + 0.15).min(1.0));
                                }))
                            )
                            Button(
                                label: "Delete… (opens a Modal)".to_string(),
                                on_press: Some(Callback::new(move |()| sm_open.set(true)))
                            )
                        }
                    })
                } else {
                    Some(element! {
                        View(flex_direction: FlexDirection::Column, gap: 1, border_style: BorderStyle::Round, padding: 1) {
                            Table(
                                headers: vec!["widget".to_string(), "kind".to_string()],
                                rows: vec![
                                    vec!["Button".to_string(), "focusable".to_string()],
                                    vec!["TextInput".to_string(), "focusable".to_string()],
                                    vec!["Spinner".to_string(), "static".to_string()],
                                ],
                                widths: vec![]
                            )
                        }
                    })
                }.into_iter())

                Divider(label: "status".to_string())

                View(flex_direction: FlexDirection::Row, gap: 2) {
                    Spinner(label: "working".to_string())
                    Badge(label: "beta".to_string(), tone: Tone::Accent)
                    Badge(label: "3 issues".to_string(), tone: Tone::Danger)
                }

                ProgressBar(value: progress.get(), width: 24, animate: true)

                GradientText(content: "gradients ship for free".to_string(), from: Some(Color::Rgb(124, 58, 237)), to: Some(Color::Rgb(34, 197, 94)))

                Tooltip(message: "overlay anchor pins these to a screen corner".to_string(), anchor: ntui::Anchor::TopRight)
                Toast(message: "Connected.".to_string(), duration: None)

                #(if show_modal.get() {
                    Some(element! {
                        Modal(
                            title: "Delete this item?".to_string(),
                            message: "This action can't be undone.".to_string(),
                            on_close: Some(Callback::new(move |()| sm_close.set(false)))
                        )
                    })
                } else {
                    None
                }.into_iter())
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(App)).await
}
