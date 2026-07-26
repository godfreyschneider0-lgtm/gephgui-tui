mod config;
mod debug;
mod nodes;
mod plus;
mod status;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Tabs},
};

use crate::state::{AppState, TabIdx};

pub fn draw_ui(f: &mut ratatui::Frame, state: &mut AppState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.area());

    let titles = vec!["1: Status", "2: Nodes", "3: Config", "4: Debug", "5: Plus"]
        .into_iter()
        .map(|t| Line::from(t))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .select(match state.tab {
            TabIdx::Status => 0,
            TabIdx::Nodes => 1,
            TabIdx::Config => 2,
            TabIdx::Debug => 3,
            TabIdx::Plus => 4,
        })
        .block(Block::default().title("Tabs").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        );
    f.render_widget(tabs, chunks[0]);

    match state.tab {
        TabIdx::Status => status::draw(f, state, chunks[1]),
        TabIdx::Nodes => nodes::draw(f, state, chunks[1]),
        TabIdx::Config => config::draw(f, state, chunks[1]),
        TabIdx::Debug => debug::draw(f, state, chunks[1]),
        TabIdx::Plus => plus::draw(f, state, chunks[1]),
    }

    let controls = Paragraph::new("Press 's' Start | 'x' Stop | 'q' Quit | '1'-'5' Tabs").block(
        Block::default()
            .title("Global Controls")
            .borders(Borders::ALL),
    );
    f.render_widget(controls, chunks[2]);
}
