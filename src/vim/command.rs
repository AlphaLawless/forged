/// Text-mutation commands. Each function takes `&mut Buffer` and applies
/// a single editing operation. No mode logic lives here — Buffer::apply_*
/// decides when to call these.
use super::buffer::Buffer;

/// Insert a character at the current cursor position and advance the cursor.
pub fn insert_char(buf: &mut Buffer, c: char) {
    let row = buf.cursor.row;
    let col = buf.cursor.col;
    let mut chars: Vec<char> = buf.lines[row].chars().collect();
    chars.insert(col, c);
    buf.lines[row] = chars.into_iter().collect();
    buf.cursor.col += 1;
}

/// Split the current line at the cursor, creating a new line below.
pub fn insert_newline(buf: &mut Buffer) {
    let row = buf.cursor.row;
    let col = buf.cursor.col;
    let chars: Vec<char> = buf.lines[row].chars().collect();
    let before: String = chars[..col].iter().collect();
    let after: String = chars[col..].iter().collect();
    buf.lines[row] = before;
    buf.lines.insert(row + 1, after);
    buf.cursor.row += 1;
    buf.cursor.col = 0;
}

/// Delete the character before the cursor.
/// At col 0 with a previous line: merge current line into the previous one.
pub fn backspace(buf: &mut Buffer) {
    let row = buf.cursor.row;
    let col = buf.cursor.col;

    if col > 0 {
        let mut chars: Vec<char> = buf.lines[row].chars().collect();
        chars.remove(col - 1);
        buf.lines[row] = chars.into_iter().collect();
        buf.cursor.col -= 1;
    } else if row > 0 {
        let current = buf.lines.remove(row);
        let prev_len = buf.lines[row - 1].chars().count();
        buf.lines[row - 1].push_str(&current);
        buf.cursor.row -= 1;
        buf.cursor.col = prev_len;
    }
}

/// Delete the character under the cursor (vim 'x').
/// Clamps the cursor if it now sits past the end of the line.
pub fn delete_char(buf: &mut Buffer) {
    let row = buf.cursor.row;
    let col = buf.cursor.col;
    let mut chars: Vec<char> = buf.lines[row].chars().collect();

    if col < chars.len() {
        chars.remove(col);
        buf.lines[row] = chars.into_iter().collect();
        let new_len = buf.lines[row].chars().count();
        if new_len > 0 && buf.cursor.col >= new_len {
            buf.cursor.col = new_len - 1;
        }
    }
}

/// Delete the entire current line (vim 'dd').
/// Always keeps at least one (empty) line in the buffer.
pub fn delete_line(buf: &mut Buffer) {
    if buf.lines.len() == 1 {
        buf.lines[0].clear();
        buf.cursor.col = 0;
        return;
    }

    let row = buf.cursor.row;
    buf.lines.remove(row);
    buf.cursor.row = buf.cursor.row.min(buf.lines.len() - 1);

    let line_len = buf.lines[buf.cursor.row].chars().count();
    buf.cursor.col = buf.cursor.col.min(line_len.saturating_sub(1));
}

/// Open a new empty line below the current one and move the cursor there.
pub fn open_line_below(buf: &mut Buffer) {
    let row = buf.cursor.row;
    buf.lines.insert(row + 1, String::new());
    buf.cursor.row += 1;
    buf.cursor.col = 0;
}

/// Open a new empty line above the current one (cursor stays on new line).
pub fn open_line_above(buf: &mut Buffer) {
    let row = buf.cursor.row;
    buf.lines.insert(row, String::new());
    buf.cursor.col = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vim::buffer::{Buffer, Cursor};

    fn buf_at(text: &str, row: usize, col: usize) -> Buffer {
        let mut b = Buffer::new(text);
        b.cursor = Cursor { row, col };
        b
    }

    #[test]
    fn test_cmd_insert_char_mid_string() {
        let mut b = buf_at("hllo", 0, 1);
        insert_char(&mut b, 'e');
        assert_eq!(b.lines[0], "hello");
        assert_eq!(b.cursor.col, 2);
    }

    #[test]
    fn test_cmd_backspace_removes_prev_char() {
        let mut b = buf_at("hello", 0, 3);
        backspace(&mut b);
        assert_eq!(b.lines[0], "helo");
        assert_eq!(b.cursor.col, 2);
    }

    #[test]
    fn test_cmd_backspace_at_col_zero_merges_lines() {
        let mut b = buf_at("foo\nbar", 1, 0);
        backspace(&mut b);
        assert_eq!(b.lines.len(), 1);
        assert_eq!(b.lines[0], "foobar");
        assert_eq!(b.cursor, Cursor { row: 0, col: 3 });
    }

    #[test]
    fn test_cmd_delete_char_removes_under_cursor() {
        let mut b = buf_at("hello", 0, 1); // cursor on 'e'
        delete_char(&mut b);
        assert_eq!(b.lines[0], "hllo");
    }

    #[test]
    fn test_cmd_delete_line_single_clears() {
        let mut b = Buffer::new("only line");
        delete_line(&mut b);
        assert_eq!(b.lines, vec![""]);
    }

    #[test]
    fn test_cmd_delete_line_removes_and_clamps_row() {
        let mut b = buf_at("a\nb\nc", 2, 0); // on 'c', last line
        delete_line(&mut b);
        assert_eq!(b.lines, vec!["a", "b"]);
        assert_eq!(b.cursor.row, 1); // clamped to new last line
    }

    #[test]
    fn test_cmd_insert_newline_splits_at_cursor() {
        let mut b = buf_at("hello world", 0, 5); // between 'o' and ' '
        insert_newline(&mut b);
        assert_eq!(b.lines[0], "hello");
        assert_eq!(b.lines[1], " world");
        assert_eq!(b.cursor, Cursor { row: 1, col: 0 });
    }

    #[test]
    fn test_cmd_open_line_below_inserts_empty() {
        let mut b = Buffer::new("hello");
        open_line_below(&mut b);
        assert_eq!(b.lines.len(), 2);
        assert_eq!(b.lines[1], "");
        assert_eq!(b.cursor.row, 1);
    }

    #[test]
    fn test_cmd_open_line_above_inserts_at_current_row() {
        let mut b = buf_at("hello", 0, 2);
        open_line_above(&mut b);
        assert_eq!(b.lines[0], "");
        assert_eq!(b.lines[1], "hello");
        assert_eq!(b.cursor.row, 0);
    }
}
