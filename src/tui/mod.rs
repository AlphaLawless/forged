pub mod theme;

pub mod widgets {
    pub mod multi_select;
    pub mod select;
    pub mod text_input;
}

pub mod views {
    pub mod action_menu;
    pub mod file_picker;
}

use anyhow::Result;
use crossterm::event::Event;
use ratatui::Frame;

/// Run a blocking TUI event loop.
///
/// - `render_fn` is called on every frame (read-only access to state).
/// - `handle_fn` is called on every event; returning `Some(R)` exits the loop.
///
/// The terminal is always restored — even if an error occurs mid-loop.
pub fn run_with<S, R>(
    mut state: S,
    render_fn: impl Fn(&mut Frame, &S),
    mut handle_fn: impl FnMut(&mut S, Event) -> Option<R>,
) -> Result<R> {
    let mut terminal = ratatui::init();

    let result = (|| -> Result<R> {
        loop {
            terminal.draw(|f| render_fn(f, &state))?;
            if crossterm::event::poll(std::time::Duration::from_millis(100))? {
                let event = crossterm::event::read()?;
                if let Some(r) = handle_fn(&mut state, event) {
                    return Ok(r);
                }
            }
        }
    })();

    ratatui::restore();
    result
}
