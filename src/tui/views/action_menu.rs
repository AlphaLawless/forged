use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::tui::{
    theme,
    widgets::select::{SelectAction, SelectItem, SelectState},
};

pub struct ActionMenuState<T: Clone> {
    message: String,
    select: SelectState<T>,
}

impl<T: Clone> ActionMenuState<T> {
    pub fn new(message: impl Into<String>, items: Vec<SelectItem<T>>) -> Self {
        let message = message.into();
        Self {
            message,
            select: SelectState::new("", items),
        }
    }

    pub fn handle_event(&mut self, event: Event) -> Option<Option<T>> {
        match self.select.handle_event(event) {
            Some(SelectAction::Picked(v)) => Some(Some(v)),
            Some(SelectAction::Cancelled) => Some(None),
            None => None,
        }
    }
}

pub fn render<T: Clone>(frame: &mut Frame, area: Rect, state: &ActionMenuState<T>) {
    let message_line_count = state.message.lines().count().max(1) as u16;
    let box_height = message_line_count + 2; // +2 for rounded border top/bottom

    let [msg_area, prompt_area, list_area, hint_area] = Layout::vertical([
        Constraint::Length(box_height),
        Constraint::Length(2), // blank + "What do you want to do?"
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Message box
    let block = Block::default()
        .title(Span::styled(" commit message ", theme::dim()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border());
    let inner = block.inner(msg_area);
    frame.render_widget(block, msg_area);
    frame.render_widget(
        Paragraph::new(state.message.as_str()).wrap(Wrap { trim: false }),
        inner,
    );

    // Prompt label
    let [_, label_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(prompt_area);
    frame.render_widget(
        Paragraph::new(Span::styled("  What do you want to do?", theme::dim())),
        label_area,
    );

    // Action list
    let items: Vec<ListItem> = state
        .select
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == state.select.selected;
            let prefix = if is_selected { "❯ " } else { "  " };
            let label_style = if is_selected {
                theme::selected()
            } else {
                theme::normal()
            };
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(item.label.clone(), label_style),
            ]))
        })
        .collect();

    let mut list_state = ListState::default().with_selected(Some(state.select.selected));
    frame.render_stateful_widget(List::new(items), list_area, &mut list_state);

    // Key hint
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", theme::primary()),
            Span::styled(" move  ", theme::dim()),
            Span::styled("Enter", theme::primary()),
            Span::styled(" confirm  ", theme::dim()),
            Span::styled("Esc", theme::primary()),
            Span::styled(" cancel", theme::dim()),
        ])),
        hint_area,
    );
}

/// Enter the TUI, show the message and action list, return the chosen value.
pub fn run<T: Clone>(message: &str, items: Vec<SelectItem<T>>) -> anyhow::Result<Option<T>> {
    let state = ActionMenuState::new(message, items);

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
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            render(frame, inner, state);
        },
        |state, event| state.handle_event(event),
    )
}
