use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use muks_adapters::{AdapterRegistry, ApplyRequest, GeneratedArtifact};
use muks_common::init_logging;
use muks_core::{MuksState, theme::derive_tokens};
use muks_installer::Installer;
use muks_syncd::SyncDaemon;
use std::ffi::OsString;
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    name = "muks",
    version,
    about = "Windows desktop customization control plane"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install(InstallCommand),
    Doctor,
    Status,
    Tui,
    Wallpaper(WallpaperCommand),
    Theme(ThemeCommand),
    Bar(ReloadCommand),
    Widgets(ReloadCommand),
    Tile(TileCommand),
    Mod(ModCommand),
    Adapter(AdapterCommand),
    Rice(RiceCommand),
    Backup(BackupCommand),
    Rollback(RollbackCommand),
    Watch(WatchCommand),
}

#[derive(Args)]
struct InstallCommand {
    #[arg(long)]
    apply: bool,
}

#[derive(Args)]
struct WallpaperCommand {
    #[command(subcommand)]
    command: WallpaperSubcommand,
}

#[derive(Subcommand)]
enum WallpaperSubcommand {
    Set { source: String },
}

#[derive(Args)]
struct ThemeCommand {
    #[command(subcommand)]
    command: ThemeSubcommand,
}

#[derive(Subcommand)]
enum ThemeSubcommand {
    Apply {
        preset: String,
        #[arg(long)]
        best_effort: bool,
    },
    Sync {
        #[arg(long)]
        best_effort: bool,
    },
}

#[derive(Args)]
struct ReloadCommand {
    #[command(subcommand)]
    command: ReloadSubcommand,
}

#[derive(Subcommand)]
enum ReloadSubcommand {
    Reload,
}

#[derive(Args)]
struct TileCommand {
    #[command(subcommand)]
    command: TileSubcommand,
}

#[derive(Subcommand)]
enum TileSubcommand {
    Start,
    Stop,
    Workspace { id: String },
}

#[derive(Args)]
struct ModCommand {
    #[command(subcommand)]
    command: ModSubcommand,
}

#[derive(Subcommand)]
enum ModSubcommand {
    Apply { profile: String },
}

#[derive(Args)]
struct AdapterCommand {
    #[command(subcommand)]
    command: AdapterSubcommand,
}

#[derive(Subcommand)]
enum AdapterSubcommand {
    List,
    Status { name: String },
    Reinstall { name: String },
}

#[derive(Args)]
struct RiceCommand {
    #[command(subcommand)]
    command: RiceSubcommand,
}

#[derive(Subcommand)]
enum RiceSubcommand {
    Save { name: String },
    Load { snapshot: String },
}

#[derive(Args)]
struct BackupCommand {
    #[command(subcommand)]
    command: BackupSubcommand,
}

#[derive(Subcommand)]
enum BackupSubcommand {
    Create,
}

#[derive(Args)]
struct RollbackCommand {
    snapshot: String,
}

#[derive(Args)]
struct WatchCommand {
    #[arg(long, default_value_t = 20)]
    iterations: usize,
    #[arg(long, default_value_t = 750)]
    interval_ms: u64,
}

pub fn run_main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();
    run_command(cli)
}

pub fn run_with_args<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    init_logging();
    let cli = Cli::try_parse_from(args).map_err(|error| anyhow!(error.to_string()))?;
    run_command(cli)
}

pub fn run_shell() -> Result<()> {
    init_logging();
    print_shell_banner();
    println!(
        "{}",
        colorize(
            "Type commands like: status, doctor, theme apply graphite --best-effort, wallpaper set nebula",
            180,
            200,
            255
        )
    );
    println!(
        "{}",
        colorize(
            "Use 'help' for tips, and 'exit' to leave the shell.",
            140,
            170,
            240
        )
    );

    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        print!("{}", colorize("mukss> ", 116, 224, 255));
        io::stdout().flush()?;
        line.clear();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "exit" | "quit" => break,
            "help" => {
                print_shell_help();
                continue;
            }
            _ => {}
        }

        let Some(mut tokens) = shlex::split(input) else {
            eprintln!(
                "{}",
                colorize("Could not parse command line.", 255, 132, 132)
            );
            continue;
        };

        if tokens
            .first()
            .map(|token| token == "muks" || token == "mukss")
            .unwrap_or(false)
        {
            let _ = tokens.remove(0);
        }

        if tokens.is_empty() {
            continue;
        }

        let mut args = Vec::with_capacity(tokens.len() + 1);
        args.push("muks".to_string());
        args.extend(tokens);

        if let Err(error) = run_with_args(args) {
            eprintln!("{} {}", colorize("Command failed:", 255, 132, 132), error);
        }
    }

    println!("{}", colorize("Goodbye from MUKS.", 140, 240, 190));
    Ok(())
}

fn run_command(cli: Cli) -> Result<()> {
    let state = MuksState::new()?;
    let registry = AdapterRegistry::new();

    match cli.command {
        Commands::Install(command) => {
            let report = Installer::new().install_all(&state.paths, command.apply)?;
            println!("Muks install plan");
            println!("winget available: {}", report.winget_available);
            for step in report.steps {
                println!(
                    "- {} | installed={} | strategy={} | attempted_install={} | success={} | {}",
                    step.tool,
                    step.installed,
                    step.strategy,
                    step.attempted_install,
                    step.install_succeeded,
                    step.note
                );
            }
        }
        Commands::Doctor => {
            let statuses = registry.detect_all(&state.paths)?;
            let mut issues = 0usize;
            for status in &statuses {
                if !status.installed {
                    issues += 1;
                    println!("- {}: {}", status.metadata.display_name, status.details);
                }
            }
            if issues == 0 {
                println!("System looks healthy. All known adapters are reachable.");
            }
        }
        Commands::Status => {
            let report = state.status_report()?;
            let adapters = registry.detect_all(&state.paths)?;
            println!("Profile: {}", report.profile_name);
            println!("Preset: {}", report.preset);
            println!("Wallpaper: {}", report.wallpaper);
            println!("Config: {}", report.config_path);
            println!("Snapshots: {}", report.snapshot_count);
            println!("Adapters:");
            for adapter in adapters {
                println!(
                    "  - {} | installed={} | {:?} | {}",
                    adapter.metadata.display_name,
                    adapter.installed,
                    adapter.health,
                    adapter.details
                );
            }
        }
        Commands::Tui => {
            muks_tui::start_tui()?;
        }
        Commands::Wallpaper(command) => match command.command {
            WallpaperSubcommand::Set { source } => {
                let config = state.set_wallpaper(&source)?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort: false,
                };
                let generated = registry.apply_all(&request)?;
                print_apply_result("Wallpaper set and synced.", &generated);
            }
        },
        Commands::Theme(command) => match command.command {
            ThemeSubcommand::Apply {
                preset,
                best_effort,
            } => {
                let config = state.set_theme_preset(&preset)?;
                let snapshot = state.create_backup("theme-apply")?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort,
                };
                let generated = registry.apply_all(&request)?;
                println!("Snapshot: {}", snapshot.id);
                print_apply_result("Theme synced.", &generated);
            }
            ThemeSubcommand::Sync { best_effort } => {
                let config = state.config()?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort,
                };
                let generated = registry.apply_all(&request)?;
                print_apply_result("Theme sync complete.", &generated);
            }
        },
        Commands::Bar(command) => match command.command {
            ReloadSubcommand::Reload => {
                let config = state.config()?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort: true,
                };
                let generated = registry.render_all(&request)?;
                print_apply_result("Bar-facing output regenerated.", &generated);
            }
        },
        Commands::Widgets(command) => match command.command {
            ReloadSubcommand::Reload => {
                let config = state.config()?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort: true,
                };
                let generated = registry.render_all(&request)?;
                print_apply_result("Widget-facing output regenerated.", &generated);
            }
        },
        Commands::Tile(command) => match command.command {
            TileSubcommand::Start => {
                println!("Komorebi start will be driven through the managed adapter path.")
            }
            TileSubcommand::Stop => {
                println!("Komorebi stop will be driven through the managed adapter path.")
            }
            TileSubcommand::Workspace { id } => println!("Workspace switch requested for {}", id),
        },
        Commands::Mod(command) => match command.command {
            ModSubcommand::Apply { profile } => {
                let config = state.set_theme_preset(&profile)?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort: true,
                };
                let generated = registry.apply_all(&request)?;
                print_apply_result("Curated mod profile rendered.", &generated);
            }
        },
        Commands::Adapter(command) => match command.command {
            AdapterSubcommand::List => {
                for metadata in registry.list_metadata(&state.paths) {
                    println!(
                        "- {} | source={} | safety={:?}",
                        metadata.display_name, metadata.install_source, metadata.safety_level
                    );
                }
            }
            AdapterSubcommand::Status { name } => {
                let status = registry.adapter_status(&name, &state.paths)?;
                println!(
                    "{} | installed={} | {:?} | {}",
                    status.metadata.display_name, status.installed, status.health, status.details
                );
            }
            AdapterSubcommand::Reinstall { name } => {
                println!("{}", registry.reinstall(&name, &state.paths)?);
            }
        },
        Commands::Rice(command) => match command.command {
            RiceSubcommand::Save { name } => {
                let snapshot = state.create_backup(&name)?;
                println!("Saved rice `{}` as snapshot `{}`", name, snapshot.id);
            }
            RiceSubcommand::Load { snapshot } => {
                let restored = state.restore_backup(&snapshot)?;
                println!("Loaded rice snapshot `{}`", restored.id);
            }
        },
        Commands::Backup(command) => match command.command {
            BackupSubcommand::Create => {
                let snapshot = state.create_backup("manual")?;
                println!("Created snapshot {}", snapshot.id);
            }
        },
        Commands::Rollback(command) => {
            let restored = state.restore_backup(&command.snapshot)?;
            println!("Rolled back to snapshot {}", restored.id);
        }
        Commands::Watch(command) => {
            let report = SyncDaemon::new()?.watch(
                command.iterations,
                std::time::Duration::from_millis(command.interval_ms),
            )?;
            println!(
                "Watch loop completed after {} iterations. Triggered apply: {}",
                report.iterations, report.triggered_apply
            );
        }
    }

    Ok(())
}

fn print_apply_result(message: &str, generated: &[GeneratedArtifact]) {
    println!("{}", message);
    for artifact in generated {
        println!("  {}:", artifact.adapter);
        for file in &artifact.files {
            println!("    generated: {}", file);
        }
        for target in &artifact.live_targets {
            println!("    live: {}", target);
        }
        for note in &artifact.notes {
            println!("    note: {}", note);
        }
    }
}

fn print_shell_help() {
    println!("{}", colorize("Common commands:", 140, 170, 240));
    println!("  status");
    println!("  doctor");
    println!("  install");
    println!("  theme apply <preset> --best-effort");
    println!("  wallpaper set <name|path|url>");
    println!("  adapter list");
    println!("  backup create");
    println!("  rice save <name>");
    println!("  rice load <snapshot-id>");
    println!("  tui");
    println!("  watch --iterations 20 --interval-ms 750");
}

fn print_shell_banner() {
    println!("{}", colorize(" __  __ _   _ _  ___ ____  ", 104, 209, 255));
    println!(
        "{}",
        colorize("|  \\/  | | | | |/ / / ___| ", 116, 224, 255)
    );
    println!(
        "{}",
        colorize("| |\\/| | | | | ' /  \\___ \\ ", 132, 240, 215)
    );
    println!(
        "{}",
        colorize("| |  | | |_| | . \\   ___) |", 255, 214, 132)
    );
    println!(
        "{}",
        colorize("|_|  |_|\\___/|_|\\_\\ |____/ ", 255, 160, 170)
    );
}

fn colorize(text: &str, r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text)
}
