use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::tui::theme;

pub struct SelectItem<T> {
    pub label: String,
    pub value: T,
    pub hint: Option<String>,
}

impl<T: Clone> SelectItem<T> {
    pub fn new(label: impl Into<String>, value: T) -> Self {
        Self {
            label: label.into(),
            value,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

pub struct SelectState<T> {
    pub items: Vec<SelectItem<T>>,
    pub selected: usize,
    pub title: String,
}

pub enum SelectAction<T> {
    Picked(T),
    Cancelled,
}

impl<T: Clone> SelectState<T> {
    pub fn new(title: impl Into<String>, items: Vec<SelectItem<T>>) -> Self {
        Self {
            title: title.into(),
            items,
            selected: 0,
        }
    }

    pub fn with_selected(mut self, idx: usize) -> Self {
        self.selected = idx.min(self.items.len().saturating_sub(1));
        self
    }

    /// Handle a key event. Returns `Some(action)` when the user confirms or cancels.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<SelectAction<T>> {
        let len = self.items.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.selected = (self.selected + 1).min(len - 1);
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.selected = 0;
                None
            }
            KeyCode::Char('G') | KeyCode::End => {
                if len > 0 {
                    self.selected = len - 1;
                }
                None
            }
            KeyCode::Enter => self
                .items
                .get(self.selected)
                .map(|item| SelectAction::Picked(item.value.clone())),
            KeyCode::Esc | KeyCode::Char('q') => Some(SelectAction::Cancelled),
            _ => None,
        }
    }

    pub fn handle_event(&mut self, event: Event) -> Option<SelectAction<T>> {
        if let Event::Key(key) = event {
            self.handle_key(key)
        } else {
            None
        }
    }
}

/// Render the select list into `area`. Does not draw any outer border — the
/// caller (a view) is responsible for layout and framing.
pub fn render<T>(frame: &mut Frame, area: Rect, state: &SelectState<T>) {
    let [list_area, hint_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .areas(area);

    let items: Vec<ListItem> = state
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == state.selected;
            let prefix = if is_selected { "❯ " } else { "  " };
            let label_style = if is_selected {
                theme::selected()
            } else {
                theme::normal()
            };

            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(item.label.clone(), label_style),
            ];
            if let Some(hint) = &item.hint {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(hint.clone(), theme::dim()));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list_state = ListState::default().with_selected(Some(state.selected));
    frame.render_stateful_widget(List::new(items), list_area, &mut list_state);

    let hint = Line::from(vec![
        Span::styled("j/k", theme::primary()),
        Span::styled(" move  ", theme::dim()),
        Span::styled("Enter", theme::primary()),
        Span::styled(" confirm  ", theme::dim()),
        Span::styled("Esc", theme::primary()),
        Span::styled(" cancel", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(hint), hint_area);
}

/// Standalone select: enters the TUI, shows the list, returns the picked value.
pub fn run<T: Clone>(
    title: &str,
    items: Vec<SelectItem<T>>,
    starting_idx: usize,
) -> anyhow::Result<Option<T>> {
    let state = SelectState::new(title, items).with_selected(starting_idx);

    crate::tui::run_with(
        state,
        |frame, state| {
            let [_, body] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Fill(1)])
                .areas(frame.area());
            let [_, inner, _] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Fill(1),
                    Constraint::Length(2),
                ])
                .areas(body);
            render(frame, inner, state);
        },
        |state, event| match state.handle_event(event) {
            Some(SelectAction::Picked(v)) => Some(Some(v)),
            Some(SelectAction::Cancelled) => Some(None),
            None => None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn make_state() -> SelectState<&'static str> {
        SelectState::new(
            "Test",
            vec![
                SelectItem::new("Foo", "foo"),
                SelectItem::new("Bar", "bar"),
                SelectItem::new("Baz", "baz"),
            ],
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_select_j_moves_down() {
        let mut s = make_state();
        assert_eq!(s.selected, 0);
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn test_select_down_arrow_moves_down() {
        let mut s = make_state();
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn test_select_k_moves_up() {
        let mut s = make_state();
        s.selected = 2;
        s.handle_key(key(KeyCode::Char('k')));
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn test_select_does_not_wrap_at_bottom() {
        let mut s = make_state();
        s.selected = 2;
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn test_select_does_not_wrap_at_top() {
        let mut s = make_state();
        s.handle_key(key(KeyCode::Char('k')));
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn test_select_enter_returns_picked() {
        let mut s = make_state();
        s.selected = 1;
        match s.handle_key(key(KeyCode::Enter)) {
            Some(SelectAction::Picked(v)) => assert_eq!(v, "bar"),
            _ => panic!("expected Picked"),
        }
    }

    #[test]
    fn test_select_esc_returns_cancelled() {
        let mut s = make_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            Some(SelectAction::Cancelled)
        ));
    }

    #[test]
    fn test_select_q_returns_cancelled() {
        let mut s = make_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('q'))),
            Some(SelectAction::Cancelled)
        ));
    }

    #[test]
    fn test_select_g_jumps_to_first() {
        let mut s = make_state();
        s.selected = 2;
        s.handle_key(key(KeyCode::Char('g')));
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn test_select_capital_g_jumps_to_last() {
        let mut s = make_state();
        s.handle_key(key(KeyCode::Char('G')));
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn test_select_with_selected_clamps() {
        let s = make_state().with_selected(99);
        assert_eq!(s.selected, 2);
    }
}
