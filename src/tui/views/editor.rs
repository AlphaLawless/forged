/// TUI adapter for the vim motor.
///
/// This is the only file allowed to import from both `crate::vim` and `crate::tui`.
/// Everything else in `src/vim/` is pure Rust with no TUI coupling.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::tui::theme;
use crate::vim::{Buffer, BufferEvent, Mode, VimKey};

/// Translate a crossterm key event into a VimKey.
/// Returns None for key events we don't handle (e.g. F-keys, media keys).
fn to_vim_key(key: KeyEvent) -> Option<VimKey> {
    // Ignore key-release events (crossterm with keyboard enhancement sends them)
    if key.kind != crossterm::event::KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE => Some(VimKey::Char(c)),
        KeyCode::Char(c) if key.modifiers == KeyModifiers::SHIFT => {
            // Shift+char arrives as the uppercase char with SHIFT modifier
            Some(VimKey::Char(c))
        }
        KeyCode::Enter => Some(VimKey::Enter),
        KeyCode::Esc => Some(VimKey::Esc),
        KeyCode::Backspace => Some(VimKey::Backspace),
        KeyCode::Delete => Some(VimKey::Delete),
        KeyCode::Up => Some(VimKey::Up),
        KeyCode::Down => Some(VimKey::Down),
        KeyCode::Left => Some(VimKey::Left),
        KeyCode::Right => Some(VimKey::Right),
        _ => None,
    }
}

pub fn render(frame: &mut Frame, area: Rect, buf: &Buffer) {
    let is_insert = buf.mode == Mode::Insert;

    let mode_label = if is_insert { " INSERT " } else { " NORMAL " };
    let mode_style = if is_insert {
        Style::default().fg(theme::PRIMARY)
    } else {
        Style::default().fg(theme::DIM)
    };

    let border_style = if is_insert {
        theme::primary()
    } else {
        theme::border()
    };

    // Outer block: title left, mode right
    let block = Block::default()
        .title(Span::styled(" edit commit message ", theme::dim()))
        .title_bottom(Span::styled(mode_label, mode_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let [editor_area, _, hint_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let inner = block.inner(editor_area);
    frame.render_widget(block, editor_area);

    // Render lines with cursor highlight
    let cursor_row = buf.cursor.row;
    let cursor_col = buf.cursor.col;

    let lines: Vec<Line> = buf
        .lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            if row != cursor_row {
                return Line::from(line.as_str());
            }
            // Highlight the cursor position
            let chars: Vec<char> = line.chars().collect();
            let before: String = chars[..cursor_col].iter().collect();
            let cursor_ch = chars
                .get(cursor_col)
                .map(|c| c.to_string())
                .unwrap_or_else(|| " ".to_string());
            let after: String = if cursor_col < chars.len() {
                chars[cursor_col + 1..].iter().collect()
            } else {
                String::new()
            };

            let cursor_style = Style::default().bg(Color::White).fg(Color::Black);
            Line::from(vec![
                Span::raw(before),
                Span::styled(cursor_ch, cursor_style),
                Span::raw(after),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);

    // Key hint
    let hint = if is_insert {
        Line::from(vec![
            Span::styled("Esc", theme::primary()),
            Span::styled(" normal mode", theme::dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", theme::primary()),
            Span::styled(" confirm  ", theme::dim()),
            Span::styled("Esc", theme::primary()),
            Span::styled(" cancel  ", theme::dim()),
            Span::styled("i", theme::primary()),
            Span::styled(" insert  ", theme::dim()),
            Span::styled("dd", theme::primary()),
            Span::styled(" del line  ", theme::dim()),
            Span::styled("u", theme::primary()),
            Span::styled(" undo", theme::dim()),
        ])
    };
    frame.render_widget(Paragraph::new(hint), hint_area);
}

/// Open an inline TUI editor pre-filled with `initial_text`.
/// Returns `Some(text)` when the user confirms (Enter in Normal mode),
/// or `None` when the user cancels (Esc in Normal mode).
pub fn run(initial_text: &str) -> anyhow::Result<Option<String>> {
    let buf = Buffer::new(initial_text);

    crate::tui::run_with(
        buf,
        |frame, buf| {
            let [_, inner, _] = Layout::horizontal([
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(2),
            ])
            .areas(frame.area());
            let [_, inner, _] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            render(frame, inner, buf);
        },
        |buf, event| {
            if let Event::Key(key) = event
                && let Some(vk) = to_vim_key(key)
            {
                return match buf.apply(vk) {
                    BufferEvent::Confirmed => Some(Some(buf.text())),
                    BufferEvent::Cancelled => Some(None),
                    _ => None,
                };
            }
            None
        },
    )
}
