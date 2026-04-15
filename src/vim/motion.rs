/// Pure cursor-position functions. None of these mutate the buffer.
/// The caller (Buffer::apply_*) sets buf.cursor to the returned Cursor.
use super::buffer::{Buffer, Cursor};

pub fn left(buf: &Buffer) -> Cursor {
    Cursor {
        row: buf.cursor.row,
        col: buf.cursor.col.saturating_sub(1),
    }
}

/// Move right in Normal mode: stops at the last character (never past it).
pub fn right_normal(buf: &Buffer) -> Cursor {
    let len = buf.current_line_len();
    Cursor {
        row: buf.cursor.row,
        col: (buf.cursor.col + 1).min(len.saturating_sub(1)),
    }
}

/// Move up, clamping col to the target line in Normal mode.
pub fn up(buf: &Buffer) -> Cursor {
    if buf.cursor.row == 0 {
        return buf.cursor;
    }
    let row = buf.cursor.row - 1;
    let max = buf.line_len(row).saturating_sub(1);
    Cursor {
        row,
        col: buf.cursor.col.min(max),
    }
}

/// Move down, clamping col to the target line in Normal mode.
pub fn down(buf: &Buffer) -> Cursor {
    let next = buf.cursor.row + 1;
    if next >= buf.lines.len() {
        return buf.cursor;
    }
    let max = buf.line_len(next).saturating_sub(1);
    Cursor {
        row: next,
        col: buf.cursor.col.min(max),
    }
}

/// Move up in Insert mode: col may equal line_len (after last char).
pub fn up_insert(buf: &Buffer) -> Cursor {
    if buf.cursor.row == 0 {
        return buf.cursor;
    }
    let row = buf.cursor.row - 1;
    Cursor {
        row,
        col: buf.cursor.col.min(buf.line_len(row)),
    }
}

/// Move down in Insert mode: col may equal line_len.
pub fn down_insert(buf: &Buffer) -> Cursor {
    let next = buf.cursor.row + 1;
    if next >= buf.lines.len() {
        return buf.cursor;
    }
    Cursor {
        row: next,
        col: buf.cursor.col.min(buf.line_len(next)),
    }
}

pub fn line_start(buf: &Buffer) -> Cursor {
    Cursor {
        row: buf.cursor.row,
        col: 0,
    }
}

/// End of line in Normal mode: last character position.
pub fn line_end(buf: &Buffer) -> Cursor {
    let row = buf.cursor.row;
    let len = buf.line_len(row);
    Cursor {
        row,
        col: len.saturating_sub(1),
    }
}

/// End of line in Insert mode: one past the last character.
pub fn line_end_insert(buf: &Buffer) -> Cursor {
    let row = buf.cursor.row;
    Cursor {
        row,
        col: buf.line_len(row),
    }
}

/// Jump to the start of the next word (skips current word then whitespace).
pub fn word_forward(buf: &Buffer) -> Cursor {
    let row = buf.cursor.row;
    let chars: Vec<char> = buf.lines[row].chars().collect();
    let mut col = buf.cursor.col;

    if col >= chars.len() {
        if row + 1 < buf.lines.len() {
            return Cursor {
                row: row + 1,
                col: 0,
            };
        }
        return buf.cursor;
    }

    // Skip the current word
    while col < chars.len() && !chars[col].is_whitespace() {
        col += 1;
    }
    // Skip whitespace
    while col < chars.len() && chars[col].is_whitespace() {
        col += 1;
    }

    if col >= chars.len() {
        if row + 1 < buf.lines.len() {
            return Cursor {
                row: row + 1,
                col: 0,
            };
        }
        return Cursor {
            row,
            col: chars.len().saturating_sub(1),
        };
    }

    Cursor { row, col }
}

/// Jump to the start of the current or previous word.
pub fn word_backward(buf: &Buffer) -> Cursor {
    let row = buf.cursor.row;
    let chars: Vec<char> = buf.lines[row].chars().collect();
    let mut col = buf.cursor.col;

    if col == 0 {
        if row == 0 {
            return buf.cursor;
        }
        let prev_len = buf.line_len(row - 1);
        return Cursor {
            row: row - 1,
            col: prev_len.saturating_sub(1),
        };
    }

    col = col.saturating_sub(1);

    // Skip whitespace backwards
    while col > 0 && chars[col].is_whitespace() {
        col -= 1;
    }
    // Skip to start of word
    while col > 0 && !chars[col - 1].is_whitespace() {
        col -= 1;
    }

    Cursor { row, col }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(text: &str) -> Buffer {
        Buffer::new(text)
    }

    fn buf_at(text: &str, row: usize, col: usize) -> Buffer {
        let mut b = Buffer::new(text);
        b.cursor = Cursor { row, col };
        b
    }

    #[test]
    fn test_motion_left_clamps_at_zero() {
        let b = buf("hello");
        assert_eq!(left(&b).col, 0);
    }

    #[test]
    fn test_motion_right_normal_clamps_at_last_char() {
        let b = buf_at("hi", 0, 1); // 'i' is last char
        assert_eq!(right_normal(&b).col, 1);
    }

    #[test]
    fn test_motion_up_clamps_at_first_row() {
        let b = buf("hello");
        assert_eq!(up(&b).row, 0);
    }

    #[test]
    fn test_motion_down_clamps_at_last_row() {
        let b = buf_at("a\nb", 1, 0);
        assert_eq!(down(&b).row, 1);
    }

    #[test]
    fn test_motion_line_start() {
        let b = buf_at("hello", 0, 3);
        assert_eq!(line_start(&b).col, 0);
    }

    #[test]
    fn test_motion_line_end() {
        let b = buf("hello"); // 5 chars, last at col=4
        assert_eq!(line_end(&b).col, 4);
    }

    #[test]
    fn test_motion_word_forward_skips_word_and_space() {
        let b = buf_at("foo bar", 0, 0);
        assert_eq!(word_forward(&b).col, 4); // 'b' in 'bar'
    }

    #[test]
    fn test_motion_word_backward_at_col_zero_goes_up() {
        let b = buf_at("line1\nline2", 1, 0);
        let c = word_backward(&b);
        assert_eq!(c.row, 0);
    }
}
