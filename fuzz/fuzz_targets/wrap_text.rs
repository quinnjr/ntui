#![no_main]
//! Invariant: `wrap_text` never produces a line wider than the (clamped)
//! max width, never invents newlines, and never panics — for any input.
use libfuzzer_sys::fuzz_target;
use ntui::__private::wrap_text;

fuzz_target!(|input: (String, u16)| {
    let (content, width) = input;
    let max_width = width as usize;
    let effective = max_width.max(1); // wrap_text clamps 0 → 1
    for line in wrap_text(&content, max_width) {
        assert!(
            line.chars().count() <= effective,
            "line of {} chars exceeds width {effective}: {line:?} (input {content:?})",
            line.chars().count(),
        );
        assert!(
            !line.contains('\n'),
            "wrapping invented a newline: {line:?}"
        );
    }
});
