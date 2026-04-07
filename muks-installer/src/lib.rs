use anyhow::Result;
use muks_adapters::AdapterRegistry;
use muks_common::{AppPaths, ToolName, command_exists};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallStep {
    pub tool: String,
    pub installed: bool,
    pub strategy: String,
    pub note: String,
    pub attempted_install: bool,
    pub install_succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReport {
    pub steps: Vec<InstallStep>,
    pub winget_available: bool,
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
                attempted_install = true;
                if winget_available {
                    if let Some(winget_id) = winget_package_id(status.tool) {
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
                        note = format!(
                            "No winget package id configured for {}. {}",
                            status.metadata.display_name, note
                        );
                    }
                } else {
                    note = format!("winget unavailable; install manually. {}", note);
                }
            }

            steps.push(InstallStep {
                tool: status.tool.as_str().to_string(),
                installed: status.installed,
                strategy,
                note,
                attempted_install,
                install_succeeded,
            });
        }

        Ok(InstallReport {
            steps,
            winget_available,
        })
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
