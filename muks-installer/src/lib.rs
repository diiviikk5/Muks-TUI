use anyhow::Result;
use muks_adapters::AdapterRegistry;
use muks_common::{AppPaths, ToolName, command_exists};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallStep {
    pub tool: String,
    pub installed: bool,
    pub strategy: String,
    pub note: String,
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

    pub fn install_all(&self, paths: &AppPaths) -> Result<InstallReport> {
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

            let note = if status.installed {
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

            steps.push(InstallStep {
                tool: status.tool.as_str().to_string(),
                installed: status.installed,
                strategy,
                note,
            });
        }

        Ok(InstallReport {
            steps,
            winget_available,
        })
    }
}
