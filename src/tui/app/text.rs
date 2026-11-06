//! A text field with a cursor, used by prompts and while typing into a
//! task's field.

use std::cell::Cell;
use std::ops::Range;

use super::Action;
use crate::tui::wrap::{locate, wrap_ranges};

/// Where a key moves the cursor to, or deletes up to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
}

#[derive(Debug)]
pub struct TextInput {
    text: String,
    /// Byte offset into `text`, always on a character boundary.
    cursor: usize,
    /// The width the field was last drawn at, for moving between wrapped
    /// lines. Zero until drawn: only `\n` breaks lines then.
    width: Cell<usize>,
}

impl TextInput {
    /// A field holding `text`, with the cursor at the end.
    pub fn new(text: String) -> TextInput {
        TextInput {
            cursor: text.len(),
            text,
            width: Cell::new(0),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Records the width the text is wrapped at, and returns the cursor's
    /// `(row, column)` within the wrapped lines.
    pub fn place(&self, width: usize) -> (usize, usize) {
        self.width.set(width);
        locate(&self.text, width, self.cursor)
    }

    /// Applies an editing action; false if `action` isn't one.
    pub(super) fn apply(&mut self, action: &Action) -> bool {
        match action {
            Action::Input(c) => self.insert(*c),
            Action::Backspace => self.delete(Motion::Left),
            Action::Move(motion) => self.cursor = self.target(*motion),
            Action::Delete(motion) => self.delete(*motion),
            _ => return false,
        }
        true
    }

    pub(super) fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Deletes between the cursor and where `motion` would take it.
    fn delete(&mut self, motion: Motion) {
        let target = self.target(motion);
        let range = self.cursor.min(target)..self.cursor.max(target);
        self.cursor = range.start;
        self.text.replace_range(range, "");
    }

    /// Where `motion` takes the cursor.
    fn target(&self, motion: Motion) -> usize {
        let (before, after) = self.text.split_at(self.cursor);
        match motion {
            Motion::Left => before.char_indices().next_back().map_or(0, |(i, _)| i),
            Motion::Right => after
                .chars()
                .next()
                .map_or(self.cursor, |c| self.cursor + c.len_utf8()),
            Motion::WordLeft => {
                let word_end = before.trim_end().len();
                before[..word_end]
                    .rfind(char::is_whitespace)
                    .map_or(0, |i| {
                        i + before[i..].chars().next().map_or(0, char::len_utf8)
                    })
            }
            Motion::WordRight => {
                let word = after.trim_start();
                let word_start = self.text.len() - word.len();
                word_start + word.find(char::is_whitespace).unwrap_or(word.len())
            }
            Motion::Up | Motion::Down | Motion::LineStart | Motion::LineEnd => {
                self.target_line(motion)
            }
        }
    }

    /// Where a motion along or between wrapped lines takes the cursor.
    fn target_line(&self, motion: Motion) -> usize {
        let width = match self.width.get() {
            0 => usize::MAX,
            width => width,
        };
        let lines = wrap_ranges(&self.text, width);
        let (row, column) = locate(&self.text, width, self.cursor);
        match motion {
            Motion::LineStart => lines[row].start,
            Motion::LineEnd => self.line_end(&lines, row),
            Motion::Up if row == 0 => 0,
            Motion::Up => self.at_column(&lines, row - 1, column),
            Motion::Down if row + 1 == lines.len() => self.text.len(),
            _ => self.at_column(&lines, row + 1, column),
        }
    }

    /// The offset `column` characters into line `row`, or its end if shorter.
    fn at_column(&self, lines: &[Range<usize>], row: usize, column: usize) -> usize {
        self.text[lines[row].clone()]
            .char_indices()
            .nth(column)
            .map_or_else(|| self.line_end(lines, row), |(i, _)| lines[row].start + i)
    }

    /// The last cursor position on line `row`. A wrapped line's end offset
    /// is the start of the next line, so stop one character short of it.
    fn line_end(&self, lines: &[Range<usize>], row: usize) -> usize {
        let line = &lines[row];
        if lines
            .get(row + 1)
            .is_some_and(|next| next.start == line.end)
        {
            self.text[line.clone()]
                .char_indices()
                .next_back()
                .map_or(line.start, |(i, _)| line.start + i)
        } else {
            line.end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Motion, TextInput};

    fn input(text: &str) -> TextInput {
        TextInput::new(text.to_string())
    }

    fn go(field: &mut TextInput, motion: Motion) {
        field.apply(&Action::Move(motion));
    }

    #[test]
    fn inserts_and_deletes_at_the_cursor() {
        let mut field = input("héllo");
        go(&mut field, Motion::LineStart);
        go(&mut field, Motion::Right);
        go(&mut field, Motion::Right);
        field.apply(&Action::Input('X'));
        assert_eq!(field.text(), "héXllo");
        field.apply(&Action::Backspace);
        field.apply(&Action::Backspace);
        assert_eq!(field.text(), "hllo");
        field.apply(&Action::Delete(Motion::Right));
        assert_eq!(field.text(), "hlo");

        // The ends are safe.
        go(&mut field, Motion::LineStart);
        go(&mut field, Motion::Left);
        field.apply(&Action::Backspace);
        go(&mut field, Motion::LineEnd);
        go(&mut field, Motion::Right);
        field.apply(&Action::Delete(Motion::Right));
        assert_eq!(field.text(), "hlo");
    }

    #[test]
    fn moves_and_deletes_by_word() {
        let mut field = input("fix  the sink");
        go(&mut field, Motion::WordLeft);
        field.apply(&Action::Input('_'));
        assert_eq!(field.text(), "fix  the _sink");
        field.apply(&Action::Backspace);
        go(&mut field, Motion::WordLeft);
        go(&mut field, Motion::WordLeft);
        go(&mut field, Motion::WordLeft);
        assert_eq!(field.place(100), (0, 0));
        go(&mut field, Motion::WordRight);
        assert_eq!(field.place(100), (0, 3));
        go(&mut field, Motion::WordRight);
        assert_eq!(field.place(100), (0, 8));
        field.apply(&Action::Delete(Motion::WordLeft));
        assert_eq!(field.text(), "fix   sink");
    }

    #[test]
    fn moves_between_lines() {
        let mut field = input("one two\nab\nlast");
        assert_eq!(field.place(100), (2, 4));
        go(&mut field, Motion::Up);
        assert_eq!(field.place(100), (1, 2), "clamped to the shorter line");
        go(&mut field, Motion::Up);
        assert_eq!(field.place(100), (0, 2));
        go(&mut field, Motion::Up);
        assert_eq!(field.place(100), (0, 0));
        go(&mut field, Motion::Down);
        go(&mut field, Motion::Down);
        go(&mut field, Motion::LineEnd);
        field.apply(&Action::Delete(Motion::LineStart));
        assert_eq!(field.text(), "one two\nab\n");
    }

    #[test]
    fn moves_between_wrapped_lines() {
        let mut field = input("aaaa bbbb cc");
        assert_eq!(field.place(5), (2, 2));
        go(&mut field, Motion::Up);
        assert_eq!(field.place(5), (1, 2));
        go(&mut field, Motion::LineEnd);
        assert_eq!(field.place(5), (1, 4), "stays on the wrapped line");
        go(&mut field, Motion::Down);
        assert_eq!(field.place(5), (2, 2));
    }
}
