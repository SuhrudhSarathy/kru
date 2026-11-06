//! Line wrapping. [`wrap`] is for text input, where the cursor position
//! must be known; [`wrap_display`] is for showing text that isn't edited.

use std::ops::Range;

/// Splits `text` into lines of at most `width` characters, breaking after
/// the last space when possible and at every `\n`. Always leaves room for
/// the cursor: if the last line is full, an empty line follows it.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    wrap_ranges(text, width)
        .into_iter()
        .map(|range| text[range].to_string())
        .collect()
}

/// The lines of [`wrap`], as byte ranges of `text`.
pub(crate) fn wrap_ranges(text: &str, width: usize) -> Vec<Range<usize>> {
    let is_full = |line: &Range<usize>| text[line.clone()].chars().count() == width;
    #[allow(clippy::single_range_in_vec_init)]
    let mut lines = vec![0..0];
    for (i, c) in text.char_indices() {
        if c == '\n' {
            lines.push(i + 1..i + 1);
            continue;
        }
        let line = lines.last_mut().expect("never empty");
        if is_full(line) {
            // Carry the unfinished word over to the new line.
            let start = match text[line.clone()].rfind(' ') {
                Some(space) if c != ' ' => line.start + space + 1,
                _ => line.end,
            };
            line.end = start;
            lines.push(start..i);
        }
        lines.last_mut().expect("never empty").end = i + c.len_utf8();
    }
    if lines.last().is_some_and(is_full) {
        lines.push(text.len()..text.len());
    }
    lines
}

/// The `(row, column)` of byte offset `at` within the lines of [`wrap`].
/// At a wrap point, that's the start of the next line.
pub(crate) fn locate(text: &str, width: usize, at: usize) -> (usize, usize) {
    let lines = wrap_ranges(text, width);
    let row = lines.iter().rposition(|line| line.start <= at).unwrap_or(0);
    (row, text[lines[row].start..at].chars().count())
}

/// Like [`wrap`], but for display: lines are trimmed, and lines left
/// empty (room for a cursor, or a lone space at a break) are dropped.
pub(crate) fn wrap_display(text: &str, width: usize) -> Vec<String> {
    let lines: Vec<String> = wrap(text, width)
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::{locate, wrap, wrap_display};

    #[test]
    fn wrap_breaks_at_spaces() {
        assert_eq!(wrap("", 10), [""]);
        assert_eq!(wrap("buy milk", 10), ["buy milk"]);
        assert_eq!(
            wrap("buy oat milk today", 10),
            ["buy oat ", "milk today", ""]
        );
        assert_eq!(wrap("buy oat milk now", 10), ["buy oat ", "milk now"]);
    }

    #[test]
    fn wrap_splits_long_words() {
        assert_eq!(wrap("abcdefghijkl", 5), ["abcde", "fghij", "kl"]);
        assert_eq!(wrap("abcde", 5), ["abcde", ""]);
    }

    #[test]
    fn wrap_breaks_at_newlines() {
        assert_eq!(wrap("ab\ncd", 10), ["ab", "cd"]);
        assert_eq!(
            wrap_display("one two three\nfour", 7),
            ["one two", "three", "four"]
        );
    }

    #[test]
    fn locate_finds_the_cursor_in_wrapped_lines() {
        assert_eq!(locate("", 10, 0), (0, 0));
        assert_eq!(locate("ab\ncd", 10, 2), (0, 2));
        assert_eq!(locate("ab\ncd", 10, 3), (1, 0));
        assert_eq!(locate("aaaa bbbb", 5, 5), (1, 0));
        assert_eq!(locate("abcde", 5, 5), (1, 0));
        assert_eq!(locate("é b", 10, 2), (0, 1));
    }

    #[test]
    fn wrap_display_drops_cursor_room_and_edge_spaces() {
        assert_eq!(wrap_display("", 10), [""]);
        assert_eq!(wrap_display("abcde", 5), ["abcde"]);
        assert_eq!(
            wrap_display("buy oat milk today", 10),
            ["buy oat", "milk today"]
        );
        assert_eq!(wrap_display("aaaaa bbbbb", 5), ["aaaaa", "bbbbb"]);
    }
}
