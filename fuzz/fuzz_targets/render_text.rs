#![no_main]
//! Renders arbitrary application-supplied text at an arbitrary terminal size
//! and asserts two invariants: the engine never panics, and no control
//! character ever reaches a painted cell (the escape-sequence-injection
//! guard in the paint layer). `w`/`h` are `u8` to keep the grid bounded.
use libfuzzer_sys::fuzz_target;
use ntui::testing::TestTerminal;
use ntui::{Element, Hooks, component, element};

#[derive(Clone, PartialEq, Default)]
struct ShowProps {
    text: String,
}

#[component]
fn Show(props: &ShowProps, _hooks: &mut Hooks) -> Element {
    element!(Text(content: props.text.clone()))
}

fuzz_target!(|input: (String, u8, u8)| {
    let (text, w, h) = input;
    let (w, h) = ((w as u16).max(1), (h as u16).max(1));
    let term = TestTerminal::new(w, h, element!(Show(text: text))).unwrap();
    // `frame_text` joins rows with '\n'; check each row's cells individually.
    for row in term.frame_text().split('\n') {
        for ch in row.chars() {
            assert!(
                !ch.is_control(),
                "control char {ch:?} reached a painted cell"
            );
        }
    }
});
