use crate::vim::{command, motion};

/// Input event type — owns no crossterm/ratatui types.
/// The TUI adapter converts crossterm::KeyEvent → VimKey before calling apply().
#[derive(Debug, Clone, PartialEq)]
pub enum VimKey {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
}

/// Outcome returned by Buffer::apply() so callers can react without inspecting state.
#[derive(Debug, Clone)]
pub enum BufferEvent {
    Noop,
    Modified,
    ModeChanged(Mode),
    /// User confirmed the edit (Enter in Normal mode).
    Confirmed,
    /// User cancelled (Esc in Normal mode).
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

struct Snapshot {
    lines: Vec<String>,
    cursor: Cursor,
}

pub struct Buffer {
    pub lines: Vec<String>,
    pub cursor: Cursor,
    pub mode: Mode,
    history: Vec<Snapshot>,
    /// Tracks whether the last Normal-mode key was 'd' (for 'dd' detection).
    pending_d: bool,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(|l| l.to_string()).collect()
        };
        Self {
            lines,
            cursor: Cursor { row: 0, col: 0 },
            mode: Mode::Normal,
            history: Vec::new(),
            pending_d: false,
        }
    }

    /// Reconstruct the full text by joining lines with newlines.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn current_line(&self) -> &str {
        &self.lines[self.cursor.row]
    }

    pub fn current_line_len(&self) -> usize {
        self.lines[self.cursor.row].chars().count()
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }

    /// Restore the most recent snapshot.
    pub fn undo(&mut self) {
        if let Some(snap) = self.history.pop() {
            self.lines = snap.lines;
            self.cursor = snap.cursor;
            self.mode = Mode::Normal;
        }
    }

    /// Dispatch a key event to the appropriate mode handler.
    pub fn apply(&mut self, key: VimKey) -> BufferEvent {
        match self.mode {
            Mode::Normal => self.apply_normal(key),
            Mode::Insert => self.apply_insert(key),
        }
    }

    fn push_snapshot(&mut self) {
        const MAX_HISTORY: usize = 50;
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }
        self.history.push(Snapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
        });
    }

    fn apply_normal(&mut self, key: VimKey) -> BufferEvent {
        // Two-key 'dd' sequence
        if self.pending_d {
            self.pending_d = false;
            if key == VimKey::Char('d') {
                self.push_snapshot();
                command::delete_line(self);
                return BufferEvent::Modified;
            }
            return BufferEvent::Noop;
        }

        match key {
            VimKey::Char('h') | VimKey::Left => {
                self.cursor = motion::left(self);
                BufferEvent::Noop
            }
            VimKey::Char('l') | VimKey::Right => {
                self.cursor = motion::right_normal(self);
                BufferEvent::Noop
            }
            VimKey::Char('k') | VimKey::Up => {
                self.cursor = motion::up(self);
                BufferEvent::Noop
            }
            VimKey::Char('j') | VimKey::Down => {
                self.cursor = motion::down(self);
                BufferEvent::Noop
            }
            VimKey::Char('0') => {
                self.cursor = motion::line_start(self);
                BufferEvent::Noop
            }
            VimKey::Char('$') => {
                self.cursor = motion::line_end(self);
                BufferEvent::Noop
            }
            VimKey::Char('w') => {
                self.cursor = motion::word_forward(self);
                BufferEvent::Noop
            }
            VimKey::Char('b') => {
                self.cursor = motion::word_backward(self);
                BufferEvent::Noop
            }
            VimKey::Char('x') => {
                self.push_snapshot();
                command::delete_char(self);
                BufferEvent::Modified
            }
            VimKey::Char('d') => {
                self.pending_d = true;
                BufferEvent::Noop
            }
            VimKey::Char('u') => {
                self.undo();
                BufferEvent::Modified
            }
            VimKey::Char('i') => {
                self.push_snapshot();
                self.mode = Mode::Insert;
                BufferEvent::ModeChanged(Mode::Insert)
            }
            VimKey::Char('a') => {
                self.push_snapshot();
                let len = self.current_line_len();
                self.cursor.col = (self.cursor.col + 1).min(len);
                self.mode = Mode::Insert;
                BufferEvent::ModeChanged(Mode::Insert)
            }
            VimKey::Char('A') => {
                self.push_snapshot();
                self.cursor.col = self.current_line_len();
                self.mode = Mode::Insert;
                BufferEvent::ModeChanged(Mode::Insert)
            }
            VimKey::Char('o') => {
                self.push_snapshot();
                command::open_line_below(self);
                self.mode = Mode::Insert;
                BufferEvent::ModeChanged(Mode::Insert)
            }
            VimKey::Char('O') => {
                self.push_snapshot();
                command::open_line_above(self);
                self.mode = Mode::Insert;
                BufferEvent::ModeChanged(Mode::Insert)
            }
            VimKey::Enter => BufferEvent::Confirmed,
            VimKey::Esc => BufferEvent::Cancelled,
            _ => BufferEvent::Noop,
        }
    }

    fn apply_insert(&mut self, key: VimKey) -> BufferEvent {
        match key {
            VimKey::Esc => {
                // Vim: on leaving insert, retreat cursor one position
                self.cursor.col = self.cursor.col.saturating_sub(1);
                self.mode = Mode::Normal;
                BufferEvent::ModeChanged(Mode::Normal)
            }
            VimKey::Char(c) => {
                command::insert_char(self, c);
                BufferEvent::Modified
            }
            VimKey::Enter => {
                command::insert_newline(self);
                BufferEvent::Modified
            }
            VimKey::Backspace => {
                command::backspace(self);
                BufferEvent::Modified
            }
            VimKey::Delete => {
                command::delete_char(self);
                BufferEvent::Modified
            }
            VimKey::Left => {
                self.cursor = motion::left(self);
                BufferEvent::Noop
            }
            VimKey::Right => {
                let len = self.current_line_len();
                self.cursor.col = (self.cursor.col + 1).min(len);
                BufferEvent::Noop
            }
            VimKey::Up => {
                self.cursor = motion::up_insert(self);
                BufferEvent::Noop
            }
            VimKey::Down => {
                self.cursor = motion::down_insert(self);
                BufferEvent::Noop
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_new_preserves_text() {
        let buf = Buffer::new("feat: add auth");
        assert_eq!(buf.lines, vec!["feat: add auth"]);
    }

    #[test]
    fn test_buffer_text_joins_lines() {
        let buf = Buffer::new("line1\nline2\nline3");
        assert_eq!(buf.text(), "line1\nline2\nline3");
    }

    #[test]
    fn test_buffer_mode_starts_normal() {
        let buf = Buffer::new("hello");
        assert_eq!(buf.mode, Mode::Normal);
    }

    #[test]
    fn test_buffer_i_enters_insert() {
        let mut buf = Buffer::new("hello");
        let event = buf.apply(VimKey::Char('i'));
        assert_eq!(buf.mode, Mode::Insert);
        assert!(matches!(event, BufferEvent::ModeChanged(Mode::Insert)));
    }

    #[test]
    fn test_buffer_esc_returns_to_normal_from_insert() {
        let mut buf = Buffer::new("hello");
        buf.apply(VimKey::Char('i'));
        let event = buf.apply(VimKey::Esc);
        assert_eq!(buf.mode, Mode::Normal);
        assert!(matches!(event, BufferEvent::ModeChanged(Mode::Normal)));
    }

    #[test]
    fn test_buffer_enter_in_normal_confirms() {
        let mut buf = Buffer::new("feat: add auth");
        let event = buf.apply(VimKey::Enter);
        assert!(matches!(event, BufferEvent::Confirmed));
    }

    #[test]
    fn test_buffer_esc_in_normal_cancels() {
        let mut buf = Buffer::new("feat: add auth");
        let event = buf.apply(VimKey::Esc);
        assert!(matches!(event, BufferEvent::Cancelled));
    }

    #[test]
    fn test_buffer_undo_restores_after_x() {
        let mut buf = Buffer::new("hello");
        buf.apply(VimKey::Char('x')); // delete 'h'
        assert_eq!(buf.lines[0], "ello");
        buf.apply(VimKey::Char('u')); // undo
        assert_eq!(buf.lines[0], "hello");
    }

    #[test]
    fn test_buffer_dd_deletes_line() {
        let mut buf = Buffer::new("line1\nline2");
        buf.apply(VimKey::Char('d'));
        buf.apply(VimKey::Char('d'));
        assert_eq!(buf.lines.len(), 1);
        assert_eq!(buf.lines[0], "line2");
    }

    #[test]
    fn test_buffer_new_empty_has_one_line() {
        let buf = Buffer::new("");
        assert_eq!(buf.lines, vec![""]);
        assert_eq!(buf.cursor, Cursor { row: 0, col: 0 });
    }
}
