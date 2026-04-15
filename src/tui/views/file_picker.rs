use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Span,
    widgets::Paragraph,
};

use crate::tui::widgets::multi_select::{MultiSelectAction, MultiSelectItem, MultiSelectState};
use crate::tui::{theme, widgets::multi_select};

pub struct FilePickerState {
    pub select: MultiSelectState<String>,
}

impl FilePickerState {
    pub fn new(files: &[(String, String)]) -> Self {
        let items = files
            .iter()
            .map(|(label, path)| MultiSelectItem::new(label.clone(), path.clone()).checked())
            .collect();
        Self {
            select: MultiSelectState::new(items),
        }
    }

    pub fn handle_event(&mut self, event: Event) -> Option<MultiSelectAction<String>> {
        self.select.handle_event(event)
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &FilePickerState) {
    let [header_area, list_area] = Layout::vertical([
        Constraint::Length(2), // title + spacer
        Constraint::Fill(1),
    ])
    .areas(area);

    let [_, label_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(header_area);

    frame.render_widget(
        Paragraph::new(Span::styled("  Stage files:", theme::dim())),
        label_area,
    );

    multi_select::render(frame, list_area, &state.select);
}

/// Show a file-picker TUI and return the selected file paths.
/// Returns `None` if the user cancelled.
pub fn run(files: &[(String, String)]) -> anyhow::Result<Option<Vec<String>>> {
    let state = FilePickerState::new(files);

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
        |state, event| match state.handle_event(event) {
            Some(MultiSelectAction::Confirmed(paths)) => Some(Some(paths)),
            Some(MultiSelectAction::Cancelled) => Some(None),
            None => None,
        },
    )
}
