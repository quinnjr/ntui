//! A Claude Code-style chat interface built with ntui.
//!
//! Demonstrates the pieces you'd use to build a real agent TUI: streaming
//! assistant output (`tokio::spawn` writing back through `State` handles),
//! tool-call blocks, an animated "thinking" spinner, a bordered input, and
//! interrupt-on-Esc — all on the flexbox layout + fiber reconciler.
//!
//! Run: `cargo run --example claude_code`
//! Keys: type + Enter to send · PgUp/PgDn scroll history · Esc interrupts a
//! reply (or quits when idle) · Ctrl-C twice quits.
//!
//! The transcript is an `Overflow::Scroll` box driven by `use_scroll`: it
//! auto-follows the bottom as replies stream, and PgUp/PgDn scroll back.

use std::time::Duration;

use ntui::{
    BorderStyle, Color, Dimension, Element, FlexDirection, KeyCode, KeyModifiers, Overflow, Weight,
    component, element, render,
};

/// Anthropic clay/orange accent.
const ACCENT: Color = Color::Rgb(0xD9, 0x77, 0x57);
const SPINNER: [&str; 6] = ["·", "✢", "✳", "∗", "✻", "✽"];
const VERBS: [&str; 12] = [
    "Cogitating",
    "Herding",
    "Noodling",
    "Percolating",
    "Ruminating",
    "Conjuring",
    "Finagling",
    "Schlepping",
    "Vibing",
    "Marinating",
    "Puzzling",
    "Wrangling",
];

#[derive(Clone, PartialEq)]
enum Block {
    User(String),
    Assistant(String),
    Tool {
        name: String,
        arg: String,
        result: String,
    },
}

fn block_view(key: usize, blk: Block) -> Element {
    match blk {
        Block::User(text) => element! {
            View(key: key.to_string(), flex_direction: FlexDirection::Row, gap: 1, margin: 0) {
                Text(content: ">", color: ACCENT, weight: Weight::Bold)
                Text(content: text)
            }
        },
        Block::Assistant(text) => element! {
            View(key: key.to_string(), flex_direction: FlexDirection::Row, gap: 1) {
                Text(content: "●", color: ACCENT)
                Text(content: text)
            }
        },
        Block::Tool { name, arg, result } => element! {
            View(key: key.to_string(), flex_direction: FlexDirection::Column) {
                View(flex_direction: FlexDirection::Row, gap: 1) {
                    Text(content: "●", color: Color::Green)
                    Text(content: format!("{name}({arg})"), weight: Weight::Bold)
                }
                Text(
                    content: format!("  ⎿  {}", if result.is_empty() { "…" } else { &result }),
                    color: Color::DarkGrey,
                )
            }
        },
    }
}

#[component]
fn App(hooks: &mut ntui::Hooks) -> Element {
    let messages = hooks.use_state(Vec::<Block>::new);
    let draft = hooks.use_state(String::new);
    let working = hooks.use_state(|| false);
    let frame = hooks.use_state(|| 0usize);
    let start = hooks.use_state(|| 0usize); // frame the current turn began
    let verb = hooks.use_state(|| VERBS[0]);
    let generation = hooks.use_state(|| 0u64); // bumped to cancel a turn
    let confirm_exit = hooks.use_state(|| false); // armed by a first ctrl-c
    let scroll = hooks.use_scroll(); // transcript scroll position (auto-follows)
    let app = hooks.use_app();

    // Animation clock — drives the spinner and the elapsed counter.
    let f = frame.clone();
    hooks.use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(120)).await;
            f.update(|n| *n = n.wrapping_add(1));
        }
    });

    let (m, d, w, g, vb, sf) = (
        messages.clone(),
        draft.clone(),
        working.clone(),
        generation.clone(),
        verb.clone(),
        start.clone(),
    );
    let cur_frame = frame.clone();
    let sc = scroll.clone();
    let ce = confirm_exit.clone();
    hooks.use_input(move |ev, _| {
        if ev.code == KeyCode::Char('c') && ev.modifiers.contains(KeyModifiers::CONTROL) {
            if ce.get() {
                app.exit();
            } else {
                ce.set(true);
                m.update(|ms| ms.push(Block::Assistant("(press ctrl-c again to exit)".into())));
            }
            return;
        }
        ce.set(false);

        match ev.code {
            KeyCode::Char(c) => d.update(|s| s.push(c)),
            KeyCode::Backspace => d.update(|s| {
                s.pop();
            }),
            KeyCode::PageUp => sc.scroll_by(-5),
            KeyCode::PageDown => sc.scroll_by(5),
            KeyCode::Esc => {
                if w.get() {
                    g.update(|n| *n += 1); // cancel the in-flight turn
                    w.set(false);
                    m.update(|ms| ms.push(Block::Assistant("[interrupted]".into())));
                } else {
                    app.exit();
                }
            }
            KeyCode::Enter => {
                let text = d.get();
                if text.trim().is_empty() {
                    return;
                }
                d.set(String::new());
                m.update(|ms| ms.push(Block::User(text.clone())));
                g.update(|n| *n += 1);
                let my_gen = g.get();
                w.set(true);
                sf.set(cur_frame.get());
                vb.set(VERBS[text.len() % VERBS.len()]);

                // Fake assistant turn: think → tool call → stream a reply.
                let (m2, w2, g2) = (m.clone(), w.clone(), g.clone());
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(650)).await;
                    if g2.get() != my_gen {
                        return;
                    }
                    m2.update(|ms| {
                        ms.push(Block::Tool {
                            name: "Read".into(),
                            arg: "src/main.rs".into(),
                            result: String::new(),
                        })
                    });
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if g2.get() != my_gen {
                        return;
                    }
                    m2.update(|ms| {
                        if let Some(Block::Tool { result, .. }) = ms.last_mut() {
                            *result = "Read 42 lines".into();
                        }
                    });

                    m2.update(|ms| ms.push(Block::Assistant(String::new())));
                    let reply = format!(
                        "Looking at \"{}\" — I'd start in main.rs, trace the call path, \
                     and add a focused test before touching the logic.",
                        text.trim()
                    );
                    for ch in reply.chars() {
                        tokio::time::sleep(Duration::from_millis(16)).await;
                        if g2.get() != my_gen {
                            return;
                        }
                        m2.update(|ms| {
                            if let Some(Block::Assistant(s)) = ms.last_mut() {
                                s.push(ch);
                            }
                        });
                    }
                    w2.set(false);
                });
            }
            _ => {}
        }
    });

    // The whole transcript is rendered into a scroll box; it auto-follows the
    // bottom as replies stream, and PgUp/PgDn scroll back through history.
    let transcript = messages.get();

    // Status line (spinner) only while a turn is in flight.
    let status = if working.get() {
        let secs = frame.get().saturating_sub(start.get()) * 120 / 1000;
        element! {
            View(flex_direction: FlexDirection::Row, gap: 1) {
                Text(content: SPINNER[frame.get() % SPINNER.len()], color: ACCENT)
                Text(
                    content: format!("{}… ({secs}s · esc to interrupt)", verb.get()),
                    color: Color::DarkGrey,
                )
            }
        }
    } else {
        element!(Text(content: ""))
    };

    // Blinking block cursor in the input.
    let cursor = if frame.get() % 8 < 4 { "▌" } else { " " };

    element! {
        View(flex_direction: FlexDirection::Column, width: Dimension::Percent(100.0), padding: 1) {
            View(
                flex_direction: FlexDirection::Column,
                border_style: BorderStyle::Round,
                border_color: ACCENT,
                padding: 1,
            ) {
                View(flex_direction: FlexDirection::Row, gap: 1) {
                    Text(content: "✻", color: ACCENT)
                    Text(content: "Welcome to Claude Code", weight: Weight::Bold)
                }
                Text(content: "/help for help · /status for setup", color: Color::DarkGrey)
            }

            View(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                gap: 1,
                margin: 1,
                overflow: Overflow::Scroll,
                scroll: scroll.clone(),
            ) {
                #(transcript.into_iter().enumerate().map(|(i, b)| block_view(i, b)))
            }

            #(std::iter::once(status))

            View(border_style: BorderStyle::Round, border_color: Color::DarkGrey) {
                Text(content: format!("> {}{cursor}", draft.get()))
            }

            Text(
                content: "⏵⏵ auto-accept · pgup/pgdn scroll · esc interrupt/quit",
                color: Color::DarkGrey,
            )
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(App)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntui::testing::TestTerminal;

    // Headless smoke: mounts the real component, exercises input, and checks
    // the frame is well-formed. Run with `cargo test --example claude_code`.

    #[tokio::test]
    async fn mounts_and_accepts_input() {
        let mut term = TestTerminal::new(72, 24, element!(App)).unwrap();
        assert!(term.frame_text().contains("Welcome to Claude Code"));

        for c in "hi there".chars() {
            term.send_key(KeyCode::Char(c)).unwrap();
        }
        assert!(term.frame_text().contains("> hi there"));

        term.send_key(KeyCode::Enter).unwrap();
        let frame = term.frame_text();
        assert!(frame.contains("hi there")); // echoed as a user block
        // Control chars must never reach a painted cell.
        for row in frame.split('\n') {
            assert!(row.chars().all(|c| !c.is_control()));
        }
    }
}
