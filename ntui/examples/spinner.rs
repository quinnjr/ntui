use std::time::Duration;

use ntui::{Color, FlexDirection, KeyCode, component, element, render};

const DOTS: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[component]
fn Spinner(hooks: &mut ntui::Hooks) -> ntui::Element {
    let frame = hooks.use_state(|| 0usize);
    let app = hooks.use_app();
    hooks.use_input(move |ev, _| {
        if ev.code == KeyCode::Char('q') {
            app.exit();
        }
    });
    let f = frame.clone();
    hooks.use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(80)).await;
            f.update(|n| *n = (*n + 1) % DOTS.len());
        }
    });
    element! {
        View(flex_direction: FlexDirection::Row, gap: 1, padding: 1) {
            Text(content: DOTS[frame.get()], color: Color::Yellow)
            Text(content: "Thinking… (q to quit)")
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(Spinner)).await
}
