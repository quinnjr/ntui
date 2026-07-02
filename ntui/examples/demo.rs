use std::time::Duration;

use ntui::{BorderStyle, Color, FlexDirection, KeyCode, Weight, component, element, render};

#[derive(Clone, PartialEq)]
struct Msg {
    id: usize,
    who: &'static str,
    text: String,
}

#[component]
fn Chat(hooks: &mut ntui::Hooks) -> ntui::Element {
    let messages = hooks.use_state(Vec::<Msg>::new);
    let draft = hooks.use_state(String::new);
    let next_id = hooks.use_state(|| 0usize);
    let app = hooks.use_app();

    let (m, d, n) = (messages.clone(), draft.clone(), next_id.clone());
    hooks.use_input(move |ev, _| match ev.code {
        KeyCode::Esc => app.exit(),
        KeyCode::Backspace => d.update(|s| {
            s.pop();
        }),
        KeyCode::Enter => {
            let text = d.get();
            if text.is_empty() {
                return;
            }
            d.set(String::new());
            let user_id = {
                let v = n.get();
                n.set(v + 2);
                v
            };
            m.update(|ms| {
                ms.push(Msg {
                    id: user_id,
                    who: "you",
                    text: text.clone(),
                });
                ms.push(Msg {
                    id: user_id + 1,
                    who: "claude",
                    text: String::new(),
                });
            });
            // fake streaming reply, one char at a time
            let m2 = m.clone();
            let reply = format!("You said: \"{text}\" — a fine thing to say to a demo.");
            tokio::spawn(async move {
                for ch in reply.chars() {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    m2.update(|ms| {
                        if let Some(last) = ms.last_mut() {
                            last.text.push(ch);
                        }
                    });
                }
            });
        }
        KeyCode::Char(c) => d.update(|s| s.push(c)),
        _ => {}
    });

    element! {
        View(flex_direction: FlexDirection::Column, padding: 1) {
            View(flex_direction: FlexDirection::Column, flex_grow: 1.0, gap: 1) {
                #(messages.get().into_iter().map(|msg| element! {
                    View(flex_direction: FlexDirection::Column, key: msg.id.to_string()) {
                        Text(content: msg.who, weight: Weight::Bold,
                             color: if msg.who == "you" { Color::Cyan } else { Color::Magenta })
                        Text(content: msg.text)
                    }
                }))
            }
            View(border_style: BorderStyle::Round, border_color: Color::DarkGrey) {
                Text(content: format!("> {}_", draft.get()))
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(Chat)).await
}
