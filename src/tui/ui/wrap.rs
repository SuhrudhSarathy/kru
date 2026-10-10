//! Line wrapping for text input, where the cursor position must be known.

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

#[cfg(test)]
mod tests {
    use super::wrap;

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
}
