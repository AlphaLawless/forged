use ratatui::style::{Color, Modifier, Style};

pub const PRIMARY: Color = Color::Cyan;
pub const SELECTED_BG: Color = Color::Rgb(30, 30, 50);
pub const SELECTED_FG: Color = Color::White;
pub const DIM: Color = Color::DarkGray;
pub const SUCCESS: Color = Color::Green;
pub const ERROR: Color = Color::Red;
pub const BORDER: Color = Color::Rgb(80, 80, 110);

pub fn selected() -> Style {
    Style::default()
        .bg(SELECTED_BG)
        .fg(SELECTED_FG)
        .add_modifier(Modifier::BOLD)
}

pub fn normal() -> Style {
    Style::default()
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn primary() -> Style {
    Style::default().fg(PRIMARY)
}

pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn error() -> Style {
    Style::default().fg(ERROR)
}

pub fn border() -> Style {
    Style::default().fg(BORDER)
}
