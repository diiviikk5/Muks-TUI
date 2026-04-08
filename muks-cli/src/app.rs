use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use muks_adapters::{AdapterRegistry, ApplyRequest, GeneratedArtifact};
use muks_common::{command_exists, init_logging};
use muks_core::{MuksState, theme::derive_tokens};
use muks_installer::Installer;
use muks_syncd::SyncDaemon;
use std::ffi::OsString;
use std::io::{self, Write};
use std::process::Command;

#[derive(Parser)]
#[command(
    name = "muks",
    version,
    about = "Windows desktop customization control plane"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Install(InstallCommand),
    Doctor(DoctorCommand),
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
struct DoctorCommand {
    #[arg(long)]
    repair: bool,
    #[arg(long)]
    strict: bool,
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
    Status {
        name: String,
    },
    Reinstall {
        name: String,
    },
    Configure {
        name: String,
        #[arg(long)]
        enabled: Option<bool>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        managed_path: Option<String>,
        #[arg(long)]
        clear_managed_path: bool,
    },
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

    let Some(command) = cli.command else {
        return run_shell();
    };

    match command {
        Commands::Install(command) => {
            let report = Installer::new().install_all(&state.paths, command.apply)?;
            println!("Muks install plan");
            println!("winget available: {}", report.winget_available);
            println!("generated at epoch: {}", report.generated_at_epoch);
            println!("report path: {}", report.report_path);
            for step in report.steps {
                println!(
                    "- {} | installed={} | version={} | strategy={} | attempted_install={} | success={} | {}",
                    step.tool,
                    step.installed,
                    step.detected_version.as_deref().unwrap_or("unknown"),
                    step.strategy,
                    step.attempted_install,
                    step.install_succeeded,
                    step.note
                );
            }
        }
        Commands::Doctor(command) => {
            run_doctor(&state, &registry, command.repair, command.strict)?;
        }
        Commands::Status => {
            let report = state.status_report()?;
            let config = state.config()?;
            let adapters = registry.detect_all(&state.paths)?;
            println!("Profile: {}", report.profile_name);
            println!("Preset: {}", report.preset);
            println!("Wallpaper: {}", report.wallpaper);
            println!("Config: {}", report.config_path);
            println!("Snapshots: {}", report.snapshot_count);
            println!("Adapters:");
            for adapter in adapters {
                println!(
                    "  - {} | enabled={} | installed={} | version={} | {:?} | {}",
                    adapter.metadata.display_name,
                    is_adapter_enabled(&config, adapter.tool),
                    adapter.installed,
                    adapter.version.as_deref().unwrap_or("unknown"),
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
                state.set_wallpaper(&source)?;
                let request = build_request(&state, false)?;
                let generated = registry.apply_all(&request)?;
                print_apply_result("Wallpaper set and synced.", &generated);
            }
        },
        Commands::Theme(command) => match command.command {
            ThemeSubcommand::Apply {
                preset,
                best_effort,
            } => {
                let snapshot = state.create_backup("theme-apply")?;
                state.set_theme_preset(&preset)?;
                let request = build_request(&state, best_effort)?;
                let generated = registry.apply_all(&request)?;
                println!("Snapshot: {}", snapshot.id);
                print_apply_result("Theme synced.", &generated);
            }
            ThemeSubcommand::Sync { best_effort } => {
                let request = build_request(&state, best_effort)?;
                let generated = registry.apply_all(&request)?;
                print_apply_result("Theme sync complete.", &generated);
            }
        },
        Commands::Bar(command) => match command.command {
            ReloadSubcommand::Reload => {
                let request = build_request(&state, true)?;
                let artifact = registry.apply_one("yasb", &request)?;
                print_apply_result("YASB bar config synced.", &[artifact]);
            }
        },
        Commands::Widgets(command) => match command.command {
            ReloadSubcommand::Reload => {
                let request = build_request(&state, true)?;
                let artifact = registry.apply_one("rainmeter", &request)?;
                print_apply_result("Rainmeter widgets synced.", &[artifact]);
            }
        },
        Commands::Tile(command) => match command.command {
            TileSubcommand::Start => {
                let request = build_request(&state, true)?;
                let artifact = registry.apply_one("komorebi", &request)?;
                print_apply_result("Komorebi config synced.", &[artifact]);
                if command_exists("komorebic.exe") {
                    let result = run_process("komorebic", &["start"])?;
                    println!("komorebic start: {}", result);
                } else {
                    println!(
                        "komorebic.exe not found on PATH; config was still generated and synced."
                    );
                }
            }
            TileSubcommand::Stop => {
                if command_exists("komorebic.exe") {
                    let result = run_process("komorebic", &["stop"])?;
                    println!("komorebic stop: {}", result);
                } else {
                    println!("komorebic.exe not found on PATH.");
                }
            }
            TileSubcommand::Workspace { id } => {
                if command_exists("komorebic.exe") {
                    let attempts = [
                        vec!["focus-workspace".to_string(), id.clone()],
                        vec!["workspace".to_string(), id.clone()],
                    ];
                    let mut executed = false;

                    for args in attempts {
                        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                        let result = run_process("komorebic", &refs)?;
                        if result == "ok" {
                            println!("Switched to workspace {}", id);
                            executed = true;
                            break;
                        }
                    }

                    if !executed {
                        println!(
                            "Could not switch workspace with known komorebic arguments. Try `komorebic focus-workspace {}` manually.",
                            id
                        );
                    }
                } else {
                    println!("komorebic.exe not found on PATH.");
                }
            }
        },
        Commands::Mod(command) => match command.command {
            ModSubcommand::Apply { profile } => {
                let config = state.set_windhawk_profile(&profile)?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: state.paths.clone(),
                    best_effort: true,
                };
                let artifact = registry.apply_one("windhawk", &request)?;
                print_apply_result("Curated Windhawk profile synced.", &[artifact]);
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
                let guidance = registry.doctor(&name, &state.paths)?;
                println!(
                    "{} | installed={} | version={} | {:?} | {}",
                    status.metadata.display_name,
                    status.installed,
                    status.version.as_deref().unwrap_or("unknown"),
                    status.health,
                    status.details
                );
                println!("doctor: {}", guidance);
            }
            AdapterSubcommand::Reinstall { name } => {
                println!("{}", registry.reinstall(&name, &state.paths)?);
            }
            AdapterSubcommand::Configure {
                name,
                enabled,
                profile,
                managed_path,
                clear_managed_path,
            } => {
                if enabled.is_none()
                    && profile.is_none()
                    && managed_path.is_none()
                    && !clear_managed_path
                {
                    return Err(anyhow!(
                        "no change requested; pass --enabled, --profile, --managed-path, or --clear-managed-path"
                    ));
                }

                let updated = state.configure_adapter(
                    &name,
                    enabled,
                    profile,
                    managed_path,
                    clear_managed_path,
                )?;
                println!("Updated adapter `{}` configuration.", name);
                println!(
                    "  rainmeter: enabled={} profile={} managed_path={}",
                    updated.rainmeter.enabled,
                    updated.rainmeter.profile,
                    updated.rainmeter.managed_path.as_deref().unwrap_or("none")
                );
                println!(
                    "  yasb: enabled={} profile={} managed_path={}",
                    updated.yasb.enabled,
                    updated.yasb.profile,
                    updated.yasb.managed_path.as_deref().unwrap_or("none")
                );
                println!(
                    "  komorebi: enabled={} profile={} managed_path={}",
                    updated.komorebi.enabled,
                    updated.komorebi.profile,
                    updated.komorebi.managed_path.as_deref().unwrap_or("none")
                );
                println!(
                    "  windhawk: enabled={} profile={} managed_path={}",
                    updated.windhawk.enabled,
                    updated.windhawk.profile,
                    updated.windhawk.managed_path.as_deref().unwrap_or("none")
                );
            }
        },
        Commands::Rice(command) => match command.command {
            RiceSubcommand::Save { name } => {
                let snapshot = state.create_backup(&name)?;
                println!("Saved rice `{}` as snapshot `{}`", name, snapshot.id);
            }
            RiceSubcommand::Load { snapshot } => {
                let restored = state.restore_backup(&snapshot)?;
                let request = build_request(&state, true)?;
                let generated = registry.apply_all(&request)?;
                println!("Loaded rice snapshot `{}`", restored.id);
                print_apply_result("Re-applied adapters from loaded snapshot.", &generated);
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
            let request = build_request(&state, true)?;
            let generated = registry.apply_all(&request)?;
            println!("Rolled back to snapshot {}", restored.id);
            print_apply_result("Re-applied adapters after rollback.", &generated);
        }
        Commands::Watch(command) => {
            let report = SyncDaemon::new()?.watch(
                command.iterations,
                std::time::Duration::from_millis(command.interval_ms),
            )?;
            println!(
                "Watch loop completed after {} iterations. Triggered apply: {} (count={})",
                report.iterations, report.triggered_apply, report.apply_count
            );
        }
    }

    Ok(())
}

fn run_doctor(
    state: &MuksState,
    registry: &AdapterRegistry,
    repair: bool,
    strict: bool,
) -> Result<()> {
    println!("Muks doctor report");
    let statuses = registry.detect_all(&state.paths)?;
    let mut issues = 0usize;

    for status in &statuses {
        let guidance = registry.doctor(status.tool.as_str(), &state.paths)?;
        println!(
            "- {} | installed={} | version={} | {:?}",
            status.metadata.display_name,
            status.installed,
            status.version.as_deref().unwrap_or("unknown"),
            status.health
        );
        println!("  details: {}", status.details);
        println!("  guidance: {}", guidance);
        if !status.installed {
            issues += 1;
        }
    }

    if repair {
        println!("\nRepair mode: attempting installer actions...");
        let report = Installer::new().install_all(&state.paths, true)?;
        println!("Install report: {}", report.report_path);
        for step in report.steps {
            println!(
                "  [{}] attempted={} success={} version={} {}",
                step.tool,
                step.attempted_install,
                step.install_succeeded,
                step.detected_version.as_deref().unwrap_or("unknown"),
                step.note
            );
        }

        println!("\nPost-repair health check:");
        let post = registry.detect_all(&state.paths)?;
        for status in &post {
            println!(
                "  - {} | installed={} | version={} | {:?}",
                status.metadata.display_name,
                status.installed,
                status.version.as_deref().unwrap_or("unknown"),
                status.health
            );
        }
        issues = post.iter().filter(|status| !status.installed).count();
    }

    if issues == 0 {
        println!("\nDoctor result: healthy");
        return Ok(());
    }

    println!(
        "\nDoctor result: {} adapter(s) still need attention.",
        issues
    );
    if strict {
        return Err(anyhow!(
            "doctor strict mode failed with {} issue(s)",
            issues
        ));
    }

    Ok(())
}

fn build_request(state: &MuksState, best_effort: bool) -> Result<ApplyRequest> {
    let config = state.config()?;
    Ok(ApplyRequest {
        tokens: derive_tokens(&config),
        config,
        paths: state.paths.clone(),
        best_effort,
    })
}

fn run_process(program: &str, args: &[&str]) -> Result<String> {
    let status = Command::new(program).args(args).status()?;
    Ok(if status.success() {
        "ok".to_string()
    } else {
        format!("failed (exit {:?})", status.code())
    })
}

fn is_adapter_enabled(config: &muks_core::AppConfig, tool: muks_common::ToolName) -> bool {
    match tool {
        muks_common::ToolName::Lively => true,
        muks_common::ToolName::Rainmeter => config.rainmeter.enabled,
        muks_common::ToolName::Yasb => config.yasb.enabled,
        muks_common::ToolName::Komorebi => config.komorebi.enabled,
        muks_common::ToolName::Windhawk => config.windhawk.enabled,
    }
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
    println!("  doctor --repair --strict");
    println!("  install");
    println!("  theme apply <preset> --best-effort");
    println!("  wallpaper set <name|path|url>");
    println!("  adapter list");
    println!("  adapter configure yasb --enabled false");
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
