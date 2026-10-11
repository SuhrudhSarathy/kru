//! Cleaning user input before it is stored.

use crate::utils::error::{Error, Result};

/// Longest single-line text a field accepts, in characters.
pub const MAX_TEXT: usize = 500;

/// Longest multi-line text a field accepts, in characters.
pub const MAX_LINES_TEXT: usize = 10_000;

/// Cleans free text: control characters are dropped and every run of
/// whitespace (newlines and tabs included) becomes a single space. Fails if
/// nothing is left or the text is longer than [`MAX_TEXT`]; `field` names
/// the field in the error.
pub fn text(raw: &str, field: &'static str) -> Result<String> {
    let cleaned = line(raw);
    if cleaned.is_empty() {
        return Err(Error::EmptyField(field));
    }
    check_length(cleaned, field, MAX_TEXT)
}

/// Cleans text that keeps its line breaks: each line is cleaned like
/// [`text`], blank lines at the start and end are dropped, and runs of
/// blank lines become one. Empty text is allowed (it clears the field).
/// Fails if longer than [`MAX_LINES_TEXT`].
pub fn lines(raw: &str, field: &'static str) -> Result<String> {
    let mut kept: Vec<String> = Vec::new();
    for cleaned in raw.lines().map(line) {
        let after_blank = kept.last().is_none_or(String::is_empty);
        if !cleaned.is_empty() || !after_blank {
            kept.push(cleaned);
        }
    }
    if kept.last().is_some_and(String::is_empty) {
        kept.pop();
    }
    check_length(kept.join("\n"), field, MAX_LINES_TEXT)
}

/// One line cleaned: control characters dropped, whitespace runs (including
/// any line breaks) turned into single spaces, ends trimmed.
fn line(raw: &str) -> String {
    let visible: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn check_length(text: String, field: &'static str, max: usize) -> Result<String> {
    let len = text.chars().count();
    if len > max {
        return Err(Error::TooLong { field, len, max });
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_and_control_characters_are_cleaned() {
        assert_eq!(text("  buy\tmilk \n now ", "x").unwrap(), "buy milk now");
        assert_eq!(text("bell\u{7}less", "x").unwrap(), "bellless");
    }

    #[test]
    fn empty_and_long_text_is_rejected() {
        assert!(matches!(text(" \n\t", "x"), Err(Error::EmptyField("x"))));
        let long = "a".repeat(MAX_TEXT + 1);
        assert!(matches!(text(&long, "x"), Err(Error::TooLong { .. })));
        assert!(text(&"a".repeat(MAX_TEXT), "x").is_ok());
    }

    #[test]
    fn lines_keep_their_breaks() {
        let raw = "\n\n  first\tline \n\n\n\nsecond  line\r\n\n";
        assert_eq!(lines(raw, "x").unwrap(), "first line\n\nsecond line");
        assert_eq!(lines("  \n ", "x").unwrap(), "");
        let long = "a".repeat(MAX_LINES_TEXT + 1);
        assert!(matches!(lines(&long, "x"), Err(Error::TooLong { .. })));
    }
}
