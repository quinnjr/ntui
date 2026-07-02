/// v1 limitation: width is measured as 1 column per `char`.
pub(crate) fn wrap_text(content: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    for raw in content.split('\n') {
        let mut line = String::new();
        let mut line_len = 0usize; // char length of `line`, tracked incrementally
        let mut first = true;
        for word in raw.split(' ').filter(|w| !w.is_empty()) {
            let word_len = word.chars().count();
            if first {
                line_len = push_word(&mut lines, &mut line, 0, word, max_width);
                first = false;
            } else if line_len + 1 + word_len <= max_width {
                line.push(' ');
                line_len = push_word(&mut lines, &mut line, line_len + 1, word, max_width);
            } else {
                lines.push(std::mem::take(&mut line));
                line_len = push_word(&mut lines, &mut line, 0, word, max_width);
            }
        }
        lines.push(line);
    }
    lines
}

/// Append `word` to `line` (whose current char length is `line_len`), hard-
/// breaking into full `max`-width lines while the combined length exceeds max.
/// Returns the char length of the resulting trailing `line`.
///
/// The common case (word fits without breaking) is a plain `push_str` — no
/// re-scan of the existing line — so wrapping a paragraph is linear in its
/// length rather than quadratic in line length.
fn push_word(
    lines: &mut Vec<String>,
    line: &mut String,
    line_len: usize,
    word: &str,
    max: usize,
) -> usize {
    let word_len = word.chars().count();
    if line_len + word_len <= max {
        line.push_str(word);
        return line_len + word_len;
    }
    let mut current: Vec<char> = line.chars().collect();
    current.extend(word.chars());
    while current.len() > max {
        let rest = current.split_off(max);
        lines.push(current.into_iter().collect());
        current = rest;
    }
    let len = current.len();
    *line = current.into_iter().collect();
    len
}

pub(crate) fn truncate_line(content: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let first = content.split('\n').next().unwrap_or("");
    let chars: Vec<char> = first.chars().collect();
    if chars.len() <= max_width {
        return first.to_string();
    }
    let mut s: String = chars[..max_width.saturating_sub(1)].iter().collect();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_word_boundaries() {
        assert_eq!(
            wrap_text("hello brave new world", 11),
            vec!["hello brave", "new world"]
        );
    }

    #[test]
    fn hard_breaks_overlong_words() {
        assert_eq!(wrap_text("abcdefgh", 3), vec!["abc", "def", "gh"]);
    }

    #[test]
    fn respects_embedded_newlines() {
        assert_eq!(wrap_text("a\nb", 10), vec!["a", "b"]);
    }

    #[test]
    fn wrap_empty_content_yields_one_empty_line() {
        assert_eq!(wrap_text("", 10), vec![""]);
    }

    #[test]
    fn wrap_width_one_hard_breaks_every_char() {
        assert_eq!(wrap_text("ab", 1), vec!["a", "b"]);
        // max_width 0 is clamped up to 1
        assert_eq!(wrap_text("ab", 0), vec!["a", "b"]);
    }

    #[test]
    fn wrap_word_exactly_max_width_stays_on_one_line() {
        assert_eq!(wrap_text("hello", 5), vec!["hello"]);
    }

    #[test]
    fn wrap_collapses_consecutive_spaces() {
        // split(' ') + filter(non-empty) drops the empty segment, so runs of
        // spaces collapse to a single separator.
        assert_eq!(wrap_text("a  b", 10), vec!["a b"]);
    }

    #[test]
    fn truncate_edge_widths_and_multiline() {
        assert_eq!(truncate_line("hi", 0), "");
        assert_eq!(truncate_line("hello", 1), "…");
        assert_eq!(truncate_line("multi\nline", 10), "multi");
    }

    #[test]
    fn truncate_clips_with_ellipsis() {
        assert_eq!(truncate_line("hello world", 7), "hello …");
        assert_eq!(truncate_line("hi", 7), "hi");
    }
}
