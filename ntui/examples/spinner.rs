use ntui::widgets::{Spinner, SpinnerProps};
use ntui::{KeyCode, component, element, render};

#[component]
fn App(hooks: &mut ntui::Hooks) -> ntui::Element {
    let app = hooks.use_app();
    hooks.use_input(move |ev, _| {
        if ev.code == KeyCode::Char('q') {
            app.exit();
        }
    });
    element! {
        View(padding: 1) {
            Spinner(label: "Thinking… (q to quit)".to_string())
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(App)).await
}
