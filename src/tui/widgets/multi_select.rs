use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::tui::theme;

pub struct MultiSelectItem<T> {
    pub label: String,
    pub value: T,
    pub checked: bool,
}

impl<T: Clone> MultiSelectItem<T> {
    pub fn new(label: impl Into<String>, value: T) -> Self {
        Self {
            label: label.into(),
            value,
            checked: false,
        }
    }

    pub fn checked(mut self) -> Self {
        self.checked = true;
        self
    }
}

pub struct MultiSelectState<T> {
    pub items: Vec<MultiSelectItem<T>>,
    pub cursor: usize,
    pub filter: String,
    /// Indices into `items` that match the current filter.
    pub filtered: Vec<usize>,
    pub filtering: bool,
}

pub enum MultiSelectAction<T> {
    Confirmed(Vec<T>),
    Cancelled,
}

impl<T: Clone> MultiSelectState<T> {
    pub fn new(items: Vec<MultiSelectItem<T>>) -> Self {
        let filtered = (0..items.len()).collect();
        Self {
            items,
            cursor: 0,
            filter: String::new(),
            filtered,
            filtering: false,
        }
    }

    fn rebuild_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.label.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        // Clamp cursor
        if !self.filtered.is_empty() {
            self.cursor = self.cursor.min(self.filtered.len() - 1);
        } else {
            self.cursor = 0;
        }
    }

    fn current_item_idx(&self) -> Option<usize> {
        self.filtered.get(self.cursor).copied()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<MultiSelectAction<T>> {
        if self.filtering {
            return self.handle_filter_key(key);
        }

        let len = self.filtered.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.cursor = (self.cursor + 1).min(len - 1);
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.cursor = 0;
                None
            }
            KeyCode::Char('G') | KeyCode::End => {
                if len > 0 {
                    self.cursor = len - 1;
                }
                None
            }
            KeyCode::Char(' ') => {
                if let Some(idx) = self.current_item_idx() {
                    self.items[idx].checked = !self.items[idx].checked;
                }
                None
            }
            KeyCode::Char('a') => {
                for idx in &self.filtered {
                    self.items[*idx].checked = true;
                }
                None
            }
            KeyCode::Char('n') => {
                for idx in &self.filtered {
                    self.items[*idx].checked = false;
                }
                None
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                None
            }
            KeyCode::Enter => {
                let selected: Vec<T> = self
                    .items
                    .iter()
                    .filter(|item| item.checked)
                    .map(|item| item.value.clone())
                    .collect();
                Some(MultiSelectAction::Confirmed(selected))
            }
            KeyCode::Esc | KeyCode::Char('q') => Some(MultiSelectAction::Cancelled),
            _ => None,
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> Option<MultiSelectAction<T>> {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.filtering = false;
                None
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_filter();
                None
            }
            KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE => {
                self.filter.push(c);
                self.rebuild_filter();
                None
            }
            _ => None,
        }
    }

    pub fn handle_event(&mut self, event: Event) -> Option<MultiSelectAction<T>> {
        if let Event::Key(key) = event {
            self.handle_key(key)
        } else {
            None
        }
    }
}

pub fn render<T>(frame: &mut Frame, area: Rect, state: &MultiSelectState<T>) {
    let [list_area, hint_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    let items: Vec<ListItem> = state
        .filtered
        .iter()
        .enumerate()
        .map(|(visible_i, &item_idx)| {
            let item = &state.items[item_idx];
            let is_cursor = visible_i == state.cursor;

            let checkbox = if item.checked { "[x]" } else { "[ ]" };
            let prefix = if is_cursor { "❯ " } else { "  " };

            let checkbox_style = if item.checked {
                theme::success()
            } else {
                theme::dim()
            };
            let label_style = if is_cursor {
                theme::selected()
            } else {
                theme::normal()
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(checkbox, checkbox_style),
                Span::raw(" "),
                Span::styled(item.label.clone(), label_style),
            ]))
        })
        .collect();

    let mut list_state = ListState::default().with_selected(Some(state.cursor));
    frame.render_stateful_widget(List::new(items), list_area, &mut list_state);

    let hint = if state.filtering {
        Line::from(vec![
            Span::styled("/", theme::primary()),
            Span::raw(state.filter.as_str()),
            Span::styled("_", theme::primary()),
            Span::styled("  Enter/Esc to stop filtering", theme::dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled("Space", theme::primary()),
            Span::styled(" toggle  ", theme::dim()),
            Span::styled("a", theme::primary()),
            Span::styled(" all  ", theme::dim()),
            Span::styled("n", theme::primary()),
            Span::styled(" none  ", theme::dim()),
            Span::styled("/", theme::primary()),
            Span::styled(" filter  ", theme::dim()),
            Span::styled("Enter", theme::primary()),
            Span::styled(" confirm  ", theme::dim()),
            Span::styled("Esc", theme::primary()),
            Span::styled(" cancel", theme::dim()),
        ])
    };
    frame.render_widget(Paragraph::new(hint), hint_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> MultiSelectState<&'static str> {
        MultiSelectState::new(vec![
            MultiSelectItem::new("foo.rs", "foo"),
            MultiSelectItem::new("bar.rs", "bar"),
            MultiSelectItem::new("baz.rs", "baz"),
        ])
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_multiselect_space_toggles_checked() {
        let mut s = make_state();
        assert!(!s.items[0].checked);
        s.handle_key(key(KeyCode::Char(' ')));
        assert!(s.items[0].checked);
        s.handle_key(key(KeyCode::Char(' ')));
        assert!(!s.items[0].checked);
    }

    #[test]
    fn test_multiselect_a_selects_all() {
        let mut s = make_state();
        s.handle_key(key(KeyCode::Char('a')));
        assert!(s.items.iter().all(|i| i.checked));
    }

    #[test]
    fn test_multiselect_n_deselects_all() {
        let mut s = make_state();
        s.handle_key(key(KeyCode::Char('a')));
        s.handle_key(key(KeyCode::Char('n')));
        assert!(s.items.iter().all(|i| !i.checked));
    }

    #[test]
    fn test_multiselect_enter_returns_checked() {
        let mut s = make_state();
        s.items[0].checked = true;
        s.items[2].checked = true;
        match s.handle_key(key(KeyCode::Enter)) {
            Some(MultiSelectAction::Confirmed(v)) => {
                assert_eq!(v, vec!["foo", "baz"]);
            }
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn test_multiselect_enter_empty_is_ok() {
        let mut s = make_state();
        match s.handle_key(key(KeyCode::Enter)) {
            Some(MultiSelectAction::Confirmed(v)) => assert!(v.is_empty()),
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn test_multiselect_esc_returns_cancelled() {
        let mut s = make_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            Some(MultiSelectAction::Cancelled)
        ));
    }

    #[test]
    fn test_multiselect_filter_reduces_visible() {
        let mut s = make_state();
        s.filtering = true;
        s.filter = "fo".into();
        s.rebuild_filter();
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.filtered[0], 0); // foo.rs
    }

    #[test]
    fn test_multiselect_filter_clamps_cursor() {
        let mut s = make_state();
        s.cursor = 2;
        s.filtering = true;
        s.filter = "fo".into();
        s.rebuild_filter();
        assert_eq!(s.cursor, 0); // clamped to 0 since only 1 item visible
    }

    #[test]
    fn test_multiselect_j_moves_down() {
        let mut s = make_state();
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn test_multiselect_does_not_wrap_at_bottom() {
        let mut s = make_state();
        s.cursor = 2;
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.cursor, 2);
    }
}
