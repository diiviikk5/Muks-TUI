use anyhow::{Context, Result, anyhow};
use muks_adapters::AdapterRegistry;
use muks_common::{AppPaths, ToolName, command_exists};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallStep {
    pub tool: String,
    pub installed: bool,
    pub detected_version: Option<String>,
    pub strategy: String,
    pub note: String,
    pub attempted_install: bool,
    pub install_succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReport {
    pub generated_at_epoch: u64,
    pub steps: Vec<InstallStep>,
    pub winget_available: bool,
    pub report_path: String,
}

pub struct Installer {
    registry: AdapterRegistry,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy)]
struct OfficialSource {
    owner: &'static str,
    repo: &'static str,
    asset_patterns: &'static [&'static str],
    kind: InstallKind,
}

#[derive(Debug, Clone, Copy)]
enum InstallKind {
    Msi,
    Exe(&'static [&'static str]),
}

const INSTALLER_TIMEOUT: Duration = Duration::from_secs(60);

impl Installer {
    pub fn new() -> Self {
        Self {
            registry: AdapterRegistry::new(),
        }
    }

    pub fn install_all(&self, paths: &AppPaths, apply: bool) -> Result<InstallReport> {
        let winget_available = command_exists("winget.exe");
        let statuses = self.registry.detect_all(paths)?;
        let mut steps = Vec::new();
        let client = Client::builder()
            .user_agent("muks-installer")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(600))
            .build()
            .context("failed to build HTTP client")?;

        for status in statuses {
            let strategy = if status.installed {
                "already-installed".to_string()
            } else if winget_available {
                "winget-or-upstream".to_string()
            } else {
                "official-upstream-auto".to_string()
            };

            let mut attempted_install = false;
            let mut install_succeeded = status.installed;
            let mut installed = status.installed;
            let mut detected_version = status.version.clone();

            let mut note = if status.installed {
                format!("{} is already installed.", status.metadata.display_name)
            } else {
                default_manual_note(status.tool)
            };

            if apply && !status.installed {
                if winget_available {
                    if let Some(winget_id) = winget_package_id(status.tool) {
                        attempted_install = true;
                        let result = Command::new("winget")
                            .args([
                                "install",
                                "--id",
                                winget_id,
                                "--source",
                                "winget",
                                "--accept-source-agreements",
                                "--accept-package-agreements",
                                "--silent",
                                "--disable-interactivity",
                            ])
                            .status();

                        match result {
                            Ok(exit) if exit.success() => {
                                note = format!("Installed using winget id `{}`.", winget_id);
                            }
                            Ok(exit) => {
                                note = format!(
                                    "winget install failed for `{}` with exit code {:?}. Falling back to official upstream.",
                                    winget_id,
                                    exit.code(),
                                );
                                if let Ok(fallback_note) =
                                    install_from_upstream(&client, status.tool, paths)
                                {
                                    note = format!("{} {}", note, fallback_note);
                                }
                            }
                            Err(error) => {
                                note = format!(
                                    "winget install failed for `{}`: {}. Falling back to official upstream.",
                                    winget_id, error
                                );
                                if let Ok(fallback_note) =
                                    install_from_upstream(&client, status.tool, paths)
                                {
                                    note = format!("{} {}", note, fallback_note);
                                }
                            }
                        }
                    } else {
                        match install_from_upstream(&client, status.tool, paths) {
                            Ok(message) => {
                                attempted_install = true;
                                note = message;
                            }
                            Err(error) => {
                                attempted_install = true;
                                note = format!(
                                    "Official install failed for {}: {}. {}",
                                    status.metadata.display_name,
                                    error,
                                    default_manual_note(status.tool)
                                );
                            }
                        }
                    }
                } else {
                    match install_from_upstream(&client, status.tool, paths) {
                        Ok(message) => {
                            attempted_install = true;
                            note = message;
                        }
                        Err(error) => {
                            attempted_install = true;
                            note = format!(
                                "Official install failed for {}: {}. {}",
                                status.metadata.display_name,
                                error,
                                default_manual_note(status.tool)
                            );
                        }
                    }
                }

                let refreshed = self.registry.adapter_status(status.tool.as_str(), paths)?;
                install_succeeded = refreshed.installed;
                installed = refreshed.installed;
                detected_version = refreshed.version;
                if attempted_install && !install_succeeded {
                    note = format!(
                        "{} Installer execution completed but tool is still not detected.",
                        note
                    );
                }
            }

            steps.push(InstallStep {
                tool: status.tool.as_str().to_string(),
                installed,
                detected_version,
                strategy,
                note,
                attempted_install,
                install_succeeded,
            });
        }

        let mut report = InstallReport {
            generated_at_epoch: current_epoch(),
            steps,
            winget_available,
            report_path: String::new(),
        };
        report.report_path = persist_report(paths, &report)?;
        Ok(report)
    }
}

fn winget_package_id(tool: ToolName) -> Option<&'static str> {
    match tool {
        ToolName::Lively => Some("rocksdanister.LivelyWallpaper"),
        ToolName::Rainmeter => Some("Rainmeter.Rainmeter"),
        ToolName::Yasb => None,
        ToolName::Komorebi => Some("LGUG2Z.komorebi"),
        ToolName::Windhawk => None,
    }
}

fn official_source(tool: ToolName) -> Option<OfficialSource> {
    match tool {
        ToolName::Lively => Some(OfficialSource {
            owner: "rocksdanister",
            repo: "lively",
            asset_patterns: &["setup_x86_full", ".exe"],
            kind: InstallKind::Exe(&["/SILENT", "/NORESTART", "/SP-"]),
        }),
        ToolName::Rainmeter => Some(OfficialSource {
            owner: "rainmeter",
            repo: "rainmeter",
            asset_patterns: &["rainmeter-", ".exe"],
            kind: InstallKind::Exe(&["/SILENT", "/NORESTART", "/SP-"]),
        }),
        ToolName::Yasb => Some(OfficialSource {
            owner: "amnweb",
            repo: "yasb",
            asset_patterns: &["x64.msi"],
            kind: InstallKind::Msi,
        }),
        ToolName::Komorebi => Some(OfficialSource {
            owner: "LGUG2Z",
            repo: "komorebi",
            asset_patterns: &["x86_64.msi"],
            kind: InstallKind::Msi,
        }),
        ToolName::Windhawk => Some(OfficialSource {
            owner: "ramensoftware",
            repo: "windhawk",
            asset_patterns: &["setup_offline.exe", "setup.exe"],
            kind: InstallKind::Exe(&["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"]),
        }),
    }
}

fn install_from_upstream(client: &Client, tool: ToolName, paths: &AppPaths) -> Result<String> {
    let source = official_source(tool).ok_or_else(|| anyhow!("no official source configured"))?;
    let release_url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        source.owner, source.repo
    );
    let release: GithubRelease = client
        .get(&release_url)
        .send()
        .context("failed to fetch GitHub release")?
        .error_for_status()
        .context("GitHub release endpoint returned an error")?
        .json()
        .context("failed to parse GitHub release response")?;

    let asset = choose_asset(&release.assets, source.asset_patterns)
        .ok_or_else(|| anyhow!("no matching install asset found in latest release"))?;

    let cache_dir = paths
        .cache
        .join("installers")
        .join(tool.as_str())
        .join(current_epoch().to_string());
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;
    let installer_path = cache_dir.join(&asset.name);

    download_asset(client, &asset.browser_download_url, &installer_path)?;
    run_installer(&installer_path, source.kind)?;

    Ok(format!(
        "Downloaded `{}` from {}/{} and executed installer.",
        asset.name, source.owner, source.repo
    ))
}

fn choose_asset<'a>(assets: &'a [GithubAsset], patterns: &[&str]) -> Option<&'a GithubAsset> {
    for pattern in patterns {
        let needle = pattern.to_ascii_lowercase();
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name.to_ascii_lowercase().contains(&needle))
        {
            return Some(asset);
        }
    }

    assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.ends_with(".msi") || name.ends_with(".exe")
    })
}

fn download_asset(client: &Client, url: &str, destination: &Path) -> Result<()> {
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to download {}", url))?
        .error_for_status()
        .with_context(|| format!("download returned non-success for {}", url))?;

    let mut file = fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    response
        .copy_to(&mut file)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    Ok(())
}

fn run_installer(installer_path: &Path, kind: InstallKind) -> Result<()> {
    if !installer_path.exists() {
        return Err(anyhow!(
            "installer file does not exist: {}",
            installer_path.display()
        ));
    }

    match kind {
        InstallKind::Msi => {
            let mut command = Command::new("msiexec");
            command
                .arg("/i")
                .arg(installer_path)
                .args(["/qn", "/norestart"]);
            let status = run_command_with_timeout(&mut command, INSTALLER_TIMEOUT)
                .context("failed to execute msiexec")?;
            if status.success() {
                Ok(())
            } else {
                Err(anyhow!(
                    "msi installer failed with exit code {:?}",
                    status.code()
                ))
            }
        }
        InstallKind::Exe(args) => {
            let mut command = Command::new(installer_path);
            command.args(args);
            let status =
                run_command_with_timeout(&mut command, INSTALLER_TIMEOUT).with_context(|| {
                    format!("failed to execute installer {}", installer_path.display())
                })?;
            if status.success() {
                Ok(())
            } else {
                Err(anyhow!(
                    "exe installer failed with exit code {:?}",
                    status.code()
                ))
            }
        }
    }
}

fn run_command_with_timeout(command: &mut Command, timeout: Duration) -> Result<ExitStatus> {
    let mut child = command
        .spawn()
        .context("failed to spawn installer process")?;
    let start = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to poll installer process")?
        {
            return Ok(status);
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "installer process timed out after {} seconds",
                timeout.as_secs()
            ));
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn default_manual_note(tool: ToolName) -> String {
    match tool {
        ToolName::Lively => "Use the official Lively installer or releases page.".to_string(),
        ToolName::Rainmeter => {
            "Use the official Rainmeter installer from rainmeter.net.".to_string()
        }
        ToolName::Yasb => {
            "Use the official YASB installer or MSI from yasb.dev/GitHub.".to_string()
        }
        ToolName::Komorebi => {
            "Use the official Komorebi releases and ensure the binary is on PATH.".to_string()
        }
        ToolName::Windhawk => "Use the official Windhawk installer from windhawk.org.".to_string(),
    }
}

fn persist_report(paths: &AppPaths, report: &InstallReport) -> Result<String> {
    let target = paths.logs.join("install-report.json");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let report_path = target.display().to_string();
    let mut stored = report.clone();
    stored.report_path = report_path.clone();
    fs::write(&target, serde_json::to_vec_pretty(&stored)?)?;
    Ok(report_path)
}

fn current_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
