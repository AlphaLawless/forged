use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::tui::theme;

pub struct TextInputState {
    pub label: String,
    pub value: String,
    pub cursor: usize,
    pub hint: Option<String>,
    pub error: Option<String>,
    pub masked: bool,
}

pub enum TextInputAction {
    Confirmed(String),
    Cancelled,
}

impl TextInputState {
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        let value = initial.into();
        let cursor = value.chars().count();
        Self {
            label: label.into(),
            value,
            cursor,
            hint: None,
            error: None,
            masked: false,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_masked(mut self) -> Self {
        self.masked = true;
        self
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<TextInputAction> {
        self.error = None;

        match key.code {
            KeyCode::Enter => {
                return Some(TextInputAction::Confirmed(self.value.clone()));
            }
            KeyCode::Esc => {
                return Some(TextInputAction::Cancelled);
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let chars: Vec<char> = self.value.chars().collect();
                    let mut new_chars = chars;
                    new_chars.remove(self.cursor - 1);
                    self.value = new_chars.into_iter().collect();
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                let len = self.value.chars().count();
                if self.cursor < len {
                    let chars: Vec<char> = self.value.chars().collect();
                    let mut new_chars = chars;
                    new_chars.remove(self.cursor);
                    self.value = new_chars.into_iter().collect();
                }
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                let len = self.value.chars().count();
                self.cursor = (self.cursor + 1).min(len);
            }
            KeyCode::Home | KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => {
                self.cursor = 0;
            }
            KeyCode::End | KeyCode::Char('e') if key.modifiers == KeyModifiers::CONTROL => {
                self.cursor = self.value.chars().count();
            }
            KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE => {
                let chars: Vec<char> = self.value.chars().collect();
                let mut new_chars = chars;
                new_chars.insert(self.cursor, c);
                self.value = new_chars.into_iter().collect();
                self.cursor += 1;
            }
            _ => {}
        }

        None
    }

    pub fn handle_event(&mut self, event: Event) -> Option<TextInputAction> {
        if let Event::Key(key) = event {
            self.handle_key(key)
        } else {
            None
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &TextInputState) {
    let [label_area, _, box_area, hint_area, _, key_hint_area] = Layout::vertical([
        Constraint::Length(1), // label
        Constraint::Length(1), // spacer
        Constraint::Length(3), // input box: border + 1 line + border
        Constraint::Length(1), // hint or error
        Constraint::Fill(1),   // padding
        Constraint::Length(1), // key hint
    ])
    .areas(area);

    // Label
    frame.render_widget(
        Paragraph::new(Span::styled(format!("  {}", state.label), theme::dim())),
        label_area,
    );

    // Input box with cursor indicator
    let chars: Vec<char> = state.value.chars().collect();
    let display: Vec<char> = if state.masked {
        vec!['•'; chars.len()]
    } else {
        chars.clone()
    };
    let before: String = display[..state.cursor].iter().collect();
    let cursor_char = display
        .get(state.cursor)
        .map(|c| c.to_string())
        .unwrap_or_else(|| " ".to_string());
    let after: String = if state.cursor < display.len() {
        display[state.cursor + 1..].iter().collect()
    } else {
        String::new()
    };

    let cursor_style = Style::default().bg(Color::White).fg(Color::Black);

    let input_line = Line::from(vec![
        Span::raw(before),
        Span::styled(cursor_char, cursor_style),
        Span::raw(after),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if state.error.is_some() {
            theme::error()
        } else {
            theme::border()
        });
    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);
    frame.render_widget(Paragraph::new(input_line), inner);

    // Hint or error
    if let Some(err) = &state.error {
        frame.render_widget(
            Paragraph::new(Span::styled(format!("  ⚠ {err}"), theme::error())),
            hint_area,
        );
    } else if let Some(hint) = &state.hint {
        frame.render_widget(
            Paragraph::new(Span::styled(format!("  {hint}"), theme::dim())),
            hint_area,
        );
    }

    // Key hint
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", theme::primary()),
            Span::styled(" confirm  ", theme::dim()),
            Span::styled("Esc", theme::primary()),
            Span::styled(" cancel", theme::dim()),
        ])),
        key_hint_area,
    );
}

fn run_state(state: TextInputState) -> anyhow::Result<Option<String>> {
    crate::tui::run_with(
        state,
        |frame, state| {
            let [_, inner, _] = Layout::horizontal([
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(2),
            ])
            .areas(frame.area());
            let [_, inner, _] = Layout::vertical([
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            render(frame, inner, state);
        },
        |state, event| match state.handle_event(event) {
            Some(TextInputAction::Confirmed(v)) => Some(Some(v)),
            Some(TextInputAction::Cancelled) => Some(None),
            None => None,
        },
    )
}

/// Enter the TUI with a text input prompt. Returns the entered string, or
/// `None` if the user cancelled.
pub fn run(label: &str, initial: &str, hint: &str) -> anyhow::Result<Option<String>> {
    run_state(TextInputState::new(label, initial).with_hint(hint))
}

/// Like `run`, but masks input characters with `•` (for passwords/API keys).
pub fn run_masked(label: &str, hint: &str) -> anyhow::Result<Option<String>> {
    run_state(TextInputState::new(label, "").with_hint(hint).with_masked())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn test_text_input_typing_appends() {
        let mut s = TextInputState::new("label", "");
        s.handle_key(key(KeyCode::Char('h')));
        s.handle_key(key(KeyCode::Char('i')));
        assert_eq!(s.value, "hi");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn test_text_input_backspace_removes() {
        let mut s = TextInputState::new("label", "hello");
        s.handle_key(key(KeyCode::Backspace));
        assert_eq!(s.value, "hell");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn test_text_input_left_right_moves_cursor() {
        let mut s = TextInputState::new("label", "abc");
        assert_eq!(s.cursor, 3);
        s.handle_key(key(KeyCode::Left));
        assert_eq!(s.cursor, 2);
        s.handle_key(key(KeyCode::Right));
        assert_eq!(s.cursor, 3);
    }

    #[test]
    fn test_text_input_ctrl_a_goes_to_start() {
        let mut s = TextInputState::new("label", "hello");
        s.handle_key(ctrl(KeyCode::Char('a')));
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_text_input_ctrl_e_goes_to_end() {
        let mut s = TextInputState::new("label", "hello");
        s.cursor = 0;
        s.handle_key(ctrl(KeyCode::Char('e')));
        assert_eq!(s.cursor, 5);
    }

    #[test]
    fn test_text_input_enter_confirms() {
        let mut s = TextInputState::new("label", "pt-br");
        match s.handle_key(key(KeyCode::Enter)) {
            Some(TextInputAction::Confirmed(v)) => assert_eq!(v, "pt-br"),
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn test_text_input_esc_cancels() {
        let mut s = TextInputState::new("label", "en");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            Some(TextInputAction::Cancelled)
        ));
    }

    #[test]
    fn test_text_input_insert_mid_string() {
        let mut s = TextInputState::new("label", "hllo");
        s.cursor = 1; // after 'h'
        s.handle_key(key(KeyCode::Char('e')));
        assert_eq!(s.value, "hello");
        assert_eq!(s.cursor, 2);
    }
}
