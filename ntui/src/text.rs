#![allow(dead_code)]

/// v1 limitation: width is measured as 1 column per `char`.
pub(crate) fn wrap_text(content: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    for raw in content.split('\n') {
        let mut line = String::new();
        let mut first = true;
        for word in raw.split(' ').filter(|w| !w.is_empty()) {
            let word_len = word.chars().count();
            let line_len = line.chars().count();
            if first || line_len + 1 + word_len <= max_width {
                if !first {
                    line.push(' ');
                }
                push_word(&mut lines, &mut line, word, max_width);
                first = false;
            } else {
                lines.push(std::mem::take(&mut line));
                push_word(&mut lines, &mut line, word, max_width);
            }
        }
        lines.push(line);
    }
    lines
}

/// Append `word` to `line`, hard-breaking into full lines while it exceeds max.
fn push_word(lines: &mut Vec<String>, line: &mut String, word: &str, max: usize) {
    let mut current: Vec<char> = line.chars().collect();
    current.extend(word.chars());
    while current.len() > max {
        let rest = current.split_off(max);
        lines.push(current.into_iter().collect());
        current = rest;
    }
    *line = current.into_iter().collect();
}

pub(crate) fn truncate_line(content: &str, max_width: usize) -> String {
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
    fn truncate_clips_with_ellipsis() {
        assert_eq!(truncate_line("hello world", 7), "hello …");
        assert_eq!(truncate_line("hi", 7), "hi");
    }
}
