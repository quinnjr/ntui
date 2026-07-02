use ntui::{BorderStyle, Color, FlexDirection, KeyCode, Weight, component, element, render};

#[component]
fn Counter(hooks: &mut ntui::Hooks) -> ntui::Element {
    let count = hooks.use_state(|| 0i32);
    let app = hooks.use_app();
    let c = count.clone();
    hooks.use_input(move |ev, _| match ev.code {
        KeyCode::Up => c.update(|n| *n += 1),
        KeyCode::Down => c.update(|n| *n -= 1),
        KeyCode::Char('q') => app.exit(),
        _ => {}
    });
    element! {
        View(flex_direction: FlexDirection::Column, padding: 1, border_style: BorderStyle::Round) {
            Text(content: format!("count: {}", count.get()), weight: Weight::Bold)
            Text(content: "↑/↓ to change · q to quit", color: Color::DarkGrey)
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(Counter)).await
}
