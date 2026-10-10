//! Line wrapping. [`wrap`] is for text input, where the cursor position
//! must be known; [`wrap_display`] is for showing text that isn't edited.

/// Splits `text` into lines of at most `width` characters, breaking after
/// the last space when possible. Always leaves room for the cursor: if the
/// last line is full, an empty line follows it.
pub(super) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for c in text.chars() {
        let line = lines.last_mut().expect("never empty");
        if line.chars().count() == width {
            // Carry the unfinished word over to the new line.
            let carry = match line.rfind(' ') {
                Some(i) if c != ' ' => line.split_off(i + 1),
                _ => String::new(),
            };
            lines.push(carry);
        }
        lines.last_mut().expect("never empty").push(c);
    }
    if lines
        .last()
        .is_some_and(|line| line.chars().count() == width)
    {
        lines.push(String::new());
    }
    lines
}

/// Like [`wrap`], but for display: lines are trimmed, and lines left
/// empty (room for a cursor, or a lone space at a break) are dropped.
pub(super) fn wrap_display(text: &str, width: usize) -> Vec<String> {
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
    use super::{wrap, wrap_display};

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
