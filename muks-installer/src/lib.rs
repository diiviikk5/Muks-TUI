use anyhow::Result;
use muks_adapters::AdapterRegistry;
use muks_common::{AppPaths, ToolName, command_exists};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

        for status in statuses {
            let strategy = if status.installed {
                "already-installed".to_string()
            } else if winget_available {
                "winget-or-upstream".to_string()
            } else {
                "official-upstream-manual".to_string()
            };

            let mut attempted_install = false;
            let mut install_succeeded = status.installed;

            let mut note = if status.installed {
                format!("{} is already installed.", status.metadata.display_name)
            } else {
                match status.tool {
                    ToolName::Lively => {
                        "Use the official Lively installer or releases page.".to_string()
                    }
                    ToolName::Rainmeter => {
                        "Use the official Rainmeter installer from rainmeter.net.".to_string()
                    }
                    ToolName::Yasb => {
                        "Use the official YASB installer or MSI from yasb.dev/GitHub.".to_string()
                    }
                    ToolName::Komorebi => {
                        "Use the official Komorebi releases and ensure the binary is on PATH."
                            .to_string()
                    }
                    ToolName::Windhawk => {
                        "Use the official Windhawk installer from windhawk.org.".to_string()
                    }
                }
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
                                install_succeeded = true;
                                note = format!("Installed using winget id `{}`.", winget_id);
                            }
                            Ok(exit) => {
                                install_succeeded = false;
                                note = format!(
                                    "winget install failed for `{}` with exit code {:?}. {}",
                                    winget_id,
                                    exit.code(),
                                    note
                                );
                            }
                            Err(error) => {
                                install_succeeded = false;
                                note = format!(
                                    "winget install failed for `{}`: {}. {}",
                                    winget_id, error, note
                                );
                            }
                        }
                    } else {
                        attempted_install = false;
                        note = format!(
                            "No winget package id configured for {}. {}",
                            status.metadata.display_name, note
                        );
                    }
                } else {
                    attempted_install = false;
                    note = format!("winget unavailable; install manually. {}", note);
                }
            }

            steps.push(InstallStep {
                tool: status.tool.as_str().to_string(),
                installed: status.installed,
                detected_version: status.version.clone(),
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
