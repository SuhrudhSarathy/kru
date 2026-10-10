//! Cleaning user input before it is stored.

use crate::utils::error::{Error, Result};

/// Longest text a field accepts, in characters.
pub const MAX_TEXT: usize = 500;

/// Cleans free text: control characters are dropped and every run of
/// whitespace (newlines and tabs included) becomes a single space. Fails if
/// nothing is left or the text is longer than [`MAX_TEXT`]; `field` names
/// the field in the error.
pub fn text(raw: &str, field: &'static str) -> Result<String> {
    let visible: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let cleaned = visible.split_whitespace().collect::<Vec<_>>().join(" ");

    let len = cleaned.chars().count();
    if len == 0 {
        Err(Error::EmptyField(field))
    } else if len > MAX_TEXT {
        Err(Error::TooLong {
            field,
            len,
            max: MAX_TEXT,
        })
    } else {
        Ok(cleaned)
    }
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
}
