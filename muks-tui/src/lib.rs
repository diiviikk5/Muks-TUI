use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use muks_adapters::AdapterRegistry;
use muks_core::MuksState;
use ratatui::{
    Terminal,
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::io::{self, Stdout};
use std::time::Duration;

pub fn start_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let state = MuksState::new()?;
    let summary = state.status_report()?;
    let adapters = AdapterRegistry::new().detect_all(&state.paths)?;
    let snapshots = state.list_snapshots()?;

    let result = run_app(&mut terminal, summary, adapters, snapshots.len());

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    summary: muks_core::StatusReport,
    adapters: Vec<muks_common::AdapterStatus>,
    snapshot_count: usize,
) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &summary, &adapters, snapshot_count))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn draw(
    frame: &mut Frame<'_>,
    summary: &muks_core::StatusReport,
    adapters: &[muks_common::AdapterStatus],
    snapshot_count: usize,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(9),
            Constraint::Min(10),
            Constraint::Length(6),
        ])
        .split(frame.area());

    let header_lines = vec![
        Line::from(vec![Span::styled(
            " __  __ _   _ _  ___ ____  ",
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(vec![Span::styled(
            "|  \\/  | | | | |/ / / ___| ",
            Style::default().fg(Color::LightBlue),
        )]),
        Line::from(vec![Span::styled(
            "| |\\/| | | | | ' /  \\___ \\ ",
            Style::default().fg(Color::LightGreen),
        )]),
        Line::from(vec![Span::styled(
            "| |  | | |_| | . \\   ___) |",
            Style::default().fg(Color::Yellow),
        )]),
        Line::from(vec![Span::styled(
            "|_|  |_|\\___/|_|\\_\\ |____/ ",
            Style::default().fg(Color::LightMagenta),
        )]),
        Line::from(""),
        Line::from(format!(
            "Profile: {}    Preset: {}    Wallpaper: {}",
            summary.profile_name, summary.preset, summary.wallpaper
        )),
        Line::from(format!("Config root: {}", summary.config_path)),
        Line::from("Press q to exit"),
    ];

    let header =
        Paragraph::new(header_lines).block(Block::default().title("Home").borders(Borders::ALL));
    frame.render_widget(header, layout[0]);

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(layout[1]);

    let adapter_items: Vec<ListItem<'_>> = adapters
        .iter()
        .map(|adapter| {
            ListItem::new(format!(
                "{} | installed={} | {:?}\n{}",
                adapter.metadata.display_name, adapter.installed, adapter.health, adapter.details
            ))
        })
        .collect();
    let adapter_list =
        List::new(adapter_items).block(Block::default().title("Adapters").borders(Borders::ALL));
    frame.render_widget(adapter_list, middle[0]);

    let side = Paragraph::new(format!(
        "Commands\nmuks install\nmuks doctor\nmuks theme apply <preset>\nmuks wallpaper set <source>\nmuks rice save <name>\n\nSnapshots: {}",
        snapshot_count
    ))
    .block(Block::default().title("Quick Actions").borders(Borders::ALL));
    frame.render_widget(side, middle[1]);

    let footer = Paragraph::new(
        "Design target: cute, fast, explicit, and repairable.\nThe generated state lives in %USERPROFILE%\\.muks so you can inspect everything Muks owns."
    )
    .block(Block::default().title("Notes").borders(Borders::ALL));
    frame.render_widget(footer, layout[2]);
}
