//! An **inline** chat: finished turns are committed to the terminal's real
//! scrollback (scroll them back with your mouse/terminal, like normal output),
//! while a live region at the bottom streams the in-progress reply and holds
//! the input. Contrast with `claude_code.rs`, which is fullscreen with an
//! in-app scroll region.
//!
//! Run: `cargo run --example inline_chat`
//! Keys: type + Enter to send · Esc to quit.
//!
//! `Scrollback` is `!Send`, so commits happen on the main thread: the streaming
//! task writes into `State` (which is `Send`); a `use_effect` commits the
//! finished reply to scrollback when the turn completes.

use std::time::Duration;

use ntui::{
    BorderStyle, Color, Element, FlexDirection, KeyCode, Weight, component, element, render_inline,
};

const ACCENT: Color = Color::Rgb(0xD9, 0x77, 0x57);
const SPINNER: [&str; 6] = ["·", "✢", "✳", "∗", "✻", "✽"];

fn user_line(text: &str) -> Element {
    element! {
        View(flex_direction: FlexDirection::Row, gap: 1) {
            Text(content: ">", color: ACCENT, weight: Weight::Bold)
            Text(content: text.to_string())
        }
    }
}

fn reply_line(text: &str) -> Element {
    element! {
        View(flex_direction: FlexDirection::Row, gap: 1) {
            Text(content: "●", color: ACCENT)
            Text(content: text.to_string())
        }
    }
}

#[component]
fn App(hooks: &mut ntui::Hooks) -> Element {
    let draft = hooks.use_state(String::new);
    let current = hooks.use_state(String::new); // streaming reply (live)
    let working = hooks.use_state(|| false);
    let frame = hooks.use_state(|| 0usize);
    // Set by the streaming task when a turn finishes: the final reply text.
    let finished = hooks.use_state(|| None::<String>);
    let scrollback = hooks.use_scrollback();
    let app = hooks.use_app();

    // Welcome banner → committed to scrollback once, on mount.
    let sb_welcome = scrollback.clone();
    hooks.use_effect((), move || {
        sb_welcome.commit(element! {
            View(flex_direction: FlexDirection::Column, border_style: BorderStyle::Round,
                 border_color: ACCENT, padding: 1) {
                Text(content: "✻ Welcome to Claude Code (inline)", weight: Weight::Bold)
                Text(content: "finished turns scroll into your terminal history",
                     color: Color::DarkGrey)
            }
        });
    });

    // Commit a finished reply to scrollback and clear the live region.
    let (sb, cur, fin, wrk) = (
        scrollback.clone(),
        current.clone(),
        finished.clone(),
        working.clone(),
    );
    hooks.use_effect(finished.get(), move || {
        if let Some(text) = fin.get() {
            sb.commit(reply_line(&text));
            fin.set(None);
            cur.set(String::new());
            wrk.set(false);
        }
    });

    // Spinner clock.
    let f = frame.clone();
    hooks.use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(120)).await;
            f.update(|n| *n = n.wrapping_add(1));
        }
    });

    let (sbi, d, c, w, fi) = (
        scrollback.clone(),
        draft.clone(),
        current.clone(),
        working.clone(),
        finished.clone(),
    );
    hooks.use_input(move |ev, _| match ev.code {
        KeyCode::Char(ch) => d.update(|s| s.push(ch)),
        KeyCode::Backspace => {
            d.update(|s| {
                s.pop();
            });
        }
        KeyCode::Esc => app.exit(),
        KeyCode::Enter => {
            let text = d.get();
            if text.trim().is_empty() {
                return;
            }
            d.set(String::new());
            sbi.commit(user_line(&text)); // user turn → scrollback immediately
            w.set(true);
            c.set(String::new());

            // Stream a reply into `current` (Send state); signal completion via
            // `finished` — the use_effect above commits it to scrollback.
            let (c2, fi2) = (c.clone(), fi.clone());
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let reply = format!(
                    "On \"{}\": I'd trace the call path and add a test first.",
                    text.trim()
                );
                let mut acc = String::new();
                for ch in reply.chars() {
                    tokio::time::sleep(Duration::from_millis(16)).await;
                    acc.push(ch);
                    c2.set(acc.clone());
                }
                fi2.set(Some(acc));
            });
        }
        _ => {}
    });

    let cursor = if frame.get() % 8 < 4 { "▌" } else { " " };

    // The live region: streaming reply (if any) + spinner + input + hint.
    element! {
        View(flex_direction: FlexDirection::Column) {
            #(if current.get().is_empty() {
                None
            } else {
                Some(reply_line(&current.get()))
            }.into_iter())

            #(if working.get() {
                Some(element! {
                    View(flex_direction: FlexDirection::Row, gap: 1) {
                        Text(content: SPINNER[frame.get() % SPINNER.len()], color: ACCENT)
                        Text(content: "Working… (esc to quit)", color: Color::DarkGrey)
                    }
                })
            } else {
                None
            }.into_iter())

            View(border_style: BorderStyle::Round, border_color: Color::DarkGrey) {
                Text(content: format!("> {}{cursor}", draft.get()))
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render_inline(element!(App)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntui::testing::TestTerminal;

    // The example renders inline at runtime; this smoke check just confirms the
    // component mounts and takes input (inline commit/live split is covered by
    // the runtime's own unit tests).
    #[tokio::test]
    async fn mounts_and_accepts_input() {
        let mut t = TestTerminal::new(50, 8, element!(App)).unwrap();
        for ch in "hi".chars() {
            t.send_key(KeyCode::Char(ch)).unwrap();
        }
        assert!(t.frame_text().contains("> hi"));
    }
}
