use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use muks_adapters::{AdapterRegistry, ApplyRequest};
use muks_common::{AdapterStatus, ToolName};
use muks_core::{MuksState, StatusReport, theme::derive_tokens};
use muks_installer::Installer;
use ratatui::{
    Terminal,
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io::{self, Stdout};
use std::time::Duration;

pub fn start_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = Dashboard::new()?;
    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut Dashboard) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => app.refresh()?,
                    KeyCode::Char('d') => app.doctor(),
                    KeyCode::Char('f') => app.doctor_repair()?,
                    KeyCode::Char('i') => app.install_plan()?,
                    KeyCode::Char('I') => app.install_apply()?,
                    KeyCode::Char('a') => app.apply_sync(true)?,
                    KeyCode::Char('1') => app.apply_preset("graphite")?,
                    KeyCode::Char('2') => app.apply_preset("forest")?,
                    KeyCode::Char('3') => app.apply_preset("rose")?,
                    KeyCode::Char('4') => app.apply_preset("cyber")?,
                    KeyCode::Char('5') => app.apply_preset("nebula")?,
                    KeyCode::Char('p') => app.apply_selected()?,
                    KeyCode::Char('s') => app.save_snapshot()?,
                    KeyCode::Char('u') => app.rollback_latest()?,
                    KeyCode::Char('x') => app.reinstall_selected()?,
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

struct Dashboard {
    state: MuksState,
    registry: AdapterRegistry,
    summary: StatusReport,
    adapters: Vec<AdapterStatus>,
    snapshot_count: usize,
    logs: Vec<String>,
    selected: usize,
}

impl Dashboard {
    fn new() -> Result<Self> {
        let state = MuksState::new()?;
        let registry = AdapterRegistry::new();
        let summary = state.status_report()?;
        let adapters = registry.detect_all(&state.paths)?;
        let snapshot_count = state.list_snapshots()?.len();

        Ok(Self {
            state,
            registry,
            summary,
            adapters,
            snapshot_count,
            logs: vec!["MUKS ready. Press i for install plan, a for apply sync.".to_string()],
            selected: 0,
        })
    }

    fn refresh(&mut self) -> Result<()> {
        self.summary = self.state.status_report()?;
        self.adapters = self.registry.detect_all(&self.state.paths)?;
        self.snapshot_count = self.state.list_snapshots()?.len();
        if self.selected >= self.adapters.len() {
            self.selected = self.adapters.len().saturating_sub(1);
        }
        self.log("Refreshed adapter and profile state.");
        Ok(())
    }

    fn install_plan(&mut self) -> Result<()> {
        let report = Installer::new().install_all(&self.state.paths, false)?;
        self.log(format!(
            "Install plan generated. winget available: {}",
            report.winget_available
        ));
        for step in report.steps {
            self.log(format!(
                "[{}] strategy={} installed={} {}",
                step.tool, step.strategy, step.installed, step.note
            ));
        }
        Ok(())
    }

    fn install_apply(&mut self) -> Result<()> {
        let report = Installer::new().install_all(&self.state.paths, true)?;
        self.log(format!(
            "Install apply executed. winget available: {}",
            report.winget_available
        ));
        for step in report.steps {
            self.log(format!(
                "[{}] installed={} attempted={} success={} {}",
                step.tool,
                step.installed,
                step.attempted_install,
                step.install_succeeded,
                step.note
            ));
        }
        self.refresh()?;
        Ok(())
    }

    fn doctor(&mut self) {
        let lines: Vec<String> = self
            .adapters
            .iter()
            .map(|adapter| {
                if adapter.installed {
                    format!("{}: healthy", adapter.metadata.display_name)
                } else {
                    format!("{}: {}", adapter.metadata.display_name, adapter.details)
                }
            })
            .collect();

        for line in lines {
            self.log(line);
        }
    }

    fn doctor_repair(&mut self) -> Result<()> {
        self.doctor();
        let report = Installer::new().install_all(&self.state.paths, true)?;
        self.log(format!(
            "Repair mode executed. winget available: {}",
            report.winget_available
        ));
        for step in report.steps {
            self.log(format!(
                "[{}] attempted={} success={} {}",
                step.tool, step.attempted_install, step.install_succeeded, step.note
            ));
        }
        self.refresh()?;
        Ok(())
    }

    fn apply_sync(&mut self, best_effort: bool) -> Result<()> {
        let config = self.state.config()?;
        let request = ApplyRequest {
            tokens: derive_tokens(&config),
            config,
            paths: self.state.paths.clone(),
            best_effort,
        };

        let artifacts = self.registry.apply_all(&request)?;
        self.log("Applied full adapter sync.");
        for artifact in artifacts {
            self.log(format!(
                "[{}] generated={} live_targets={}",
                artifact.adapter,
                artifact.files.len(),
                artifact.live_targets.len()
            ));
            for note in artifact.notes {
                self.log(format!("  note: {}", note));
            }
        }
        self.refresh()?;
        Ok(())
    }

    fn apply_preset(&mut self, preset: &str) -> Result<()> {
        self.state.set_theme_preset(preset)?;
        self.log(format!("Preset selected: {}", preset));
        self.apply_sync(true)
    }

    fn apply_selected(&mut self) -> Result<()> {
        let Some(tool) = self.selected_tool() else {
            return Ok(());
        };
        let config = self.state.config()?;
        let request = ApplyRequest {
            tokens: derive_tokens(&config),
            config,
            paths: self.state.paths.clone(),
            best_effort: true,
        };
        let artifact = self.registry.apply_one(tool.as_str(), &request)?;
        self.log(format!(
            "{} synced. generated={} live_targets={}",
            tool.display_name(),
            artifact.files.len(),
            artifact.live_targets.len()
        ));
        self.refresh()?;
        Ok(())
    }

    fn reinstall_selected(&mut self) -> Result<()> {
        let Some(adapter) = self.adapters.get(self.selected) else {
            return Ok(());
        };
        let tool = adapter.tool.as_str();
        let note = self.registry.reinstall(tool, &self.state.paths)?;
        self.log(format!(
            "{} reinstall: {}",
            adapter.metadata.display_name, note
        ));
        Ok(())
    }

    fn save_snapshot(&mut self) -> Result<()> {
        let snapshot = self.state.create_backup("tui-manual")?;
        self.log(format!("Snapshot created: {}", snapshot.id));
        self.refresh()?;
        Ok(())
    }

    fn rollback_latest(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.list_snapshots()?.into_iter().next() else {
            self.log("No snapshot available to rollback.");
            return Ok(());
        };

        self.state.restore_backup(&snapshot.id)?;
        self.log(format!("Restored snapshot: {}", snapshot.id));

        let config = self.state.config()?;
        let request = ApplyRequest {
            tokens: derive_tokens(&config),
            config,
            paths: self.state.paths.clone(),
            best_effort: true,
        };
        let artifacts = self.registry.apply_all(&request)?;
        self.log(format!(
            "Re-applied {} adapters after rollback.",
            artifacts.len()
        ));
        self.refresh()?;
        Ok(())
    }

    fn select_next(&mut self) {
        if self.adapters.is_empty() {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1) % self.adapters.len();
        }
    }

    fn select_prev(&mut self) {
        if self.adapters.is_empty() {
            self.selected = 0;
        } else if self.selected == 0 {
            self.selected = self.adapters.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn selected_tool(&self) -> Option<ToolName> {
        self.adapters.get(self.selected).map(|adapter| adapter.tool)
    }

    fn log<S: Into<String>>(&mut self, message: S) {
        self.logs.push(message.into());
        if self.logs.len() > 14 {
            let excess = self.logs.len() - 14;
            self.logs.drain(0..excess);
        }
    }
}

fn draw(frame: &mut Frame<'_>, app: &Dashboard) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(9),
            Constraint::Min(10),
            Constraint::Length(8),
        ])
        .split(frame.area());

    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            " __  __ _   _ _  ___ ____  ",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(Span::styled(
            "|  \\/  | | | | |/ / / ___| ",
            Style::default().fg(Color::LightBlue),
        )),
        Line::from(Span::styled(
            "| |\\/| | | | | ' /  \\___ \\ ",
            Style::default().fg(Color::LightGreen),
        )),
        Line::from(Span::styled(
            "| |  | | |_| | . \\   ___) |",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            "|_|  |_|\\___/|_|\\_\\ |____/ ",
            Style::default().fg(Color::LightMagenta),
        )),
        Line::from(""),
        Line::from(format!(
            "Profile: {} | Preset: {} | Wallpaper: {}",
            app.summary.profile_name, app.summary.preset, app.summary.wallpaper
        )),
        Line::from(format!("Config: {}", app.summary.config_path)),
    ])
    .block(
        Block::default()
            .title("MUKS Command Center")
            .borders(Borders::ALL),
    );
    frame.render_widget(header, root[0]);

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(root[1]);

    let adapter_items: Vec<ListItem<'_>> = app
        .adapters
        .iter()
        .map(|adapter| {
            ListItem::new(format!(
                "{} | installed={} | {:?}",
                adapter.metadata.display_name, adapter.installed, adapter.health
            ))
        })
        .collect();
    let mut list_state = ListState::default().with_selected(Some(app.selected));
    let adapters = List::new(adapter_items)
        .highlight_style(Style::default().bg(Color::Rgb(38, 56, 92)))
        .highlight_symbol("> ")
        .block(
            Block::default()
                .title("Adapters (j/k)")
                .borders(Borders::ALL),
        );
    frame.render_stateful_widget(adapters, middle[0], &mut list_state);

    let selected_name = app
        .selected_tool()
        .map(|tool| tool.display_name())
        .unwrap_or("None");
    let side = Paragraph::new(format!(
        "Selected: {}\nSnapshots: {}\n\nActions:\n  r  refresh\n  d  doctor\n  f  doctor repair\n  i  install plan\n  I  install apply\n  a  apply full sync\n  1-5 apply preset\n  p  apply selected\n  x  reinstall selected\n  s  snapshot create\n  u  rollback latest\n  q  quit",
        selected_name, app.snapshot_count
    ))
    .block(Block::default().title("Actions").borders(Borders::ALL));
    frame.render_widget(side, middle[1]);

    let log_lines: Vec<Line<'_>> = app
        .logs
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect();
    let logs = Paragraph::new(log_lines)
        .block(Block::default().title("Activity Log").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(logs, root[2]);
}
