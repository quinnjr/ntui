use ntui::{BorderStyle, Color, FlexDirection, KeyCode, component, element, render};

#[component]
fn TodoList(hooks: &mut ntui::Hooks) -> ntui::Element {
    let items = hooks.use_state(|| vec!["learn ntui".to_string()]);
    let next_id = hooks.use_state(|| 1usize);
    let app = hooks.use_app();
    let (i, n) = (items.clone(), next_id.clone());
    hooks.use_input(move |ev, _| match ev.code {
        KeyCode::Char('a') => {
            let id = {
                let v = n.get();
                n.set(v + 1);
                v
            };
            i.update(|items| items.push(format!("task #{id}")));
        }
        KeyCode::Char('d') => i.update(|items| {
            items.pop();
        }),
        KeyCode::Char('q') => app.exit(),
        _ => {}
    });
    element! {
        View(flex_direction: FlexDirection::Column, padding: 1, border_style: BorderStyle::Single) {
            Text(content: "a: add · d: delete last · q: quit", color: Color::DarkGrey)
            #(items.get().into_iter().map(|item| element! {
                Text(content: format!("• {item}"), key: item)
            }))
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(TodoList)).await
}
