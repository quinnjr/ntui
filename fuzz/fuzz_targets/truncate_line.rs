#![no_main]
//! Invariant: `truncate_line` never returns more than `max_width` chars,
//! stays single-line, and never panics — for any input.
use libfuzzer_sys::fuzz_target;
use ntui::__private::truncate_line;

fuzz_target!(|input: (String, u16)| {
    let (content, width) = input;
    let max_width = width as usize;
    let out = truncate_line(&content, max_width);
    assert!(
        out.chars().count() <= max_width,
        "truncated to {} chars, over width {max_width}: {out:?} (input {content:?})",
        out.chars().count(),
    );
    assert!(!out.contains('\n'), "truncation leaked a newline: {out:?}");
});
