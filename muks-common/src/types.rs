use serde::{Deserialize, Serialize};

pub type MuksResult<T> = anyhow::Result<T>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    Lively,
    Rainmeter,
    Yasb,
    Komorebi,
    Windhawk,
}

impl ToolName {
    pub const ALL: [ToolName; 5] = [
        ToolName::Lively,
        ToolName::Rainmeter,
        ToolName::Yasb,
        ToolName::Komorebi,
        ToolName::Windhawk,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ToolName::Lively => "lively",
            ToolName::Rainmeter => "rainmeter",
            ToolName::Yasb => "yasb",
            ToolName::Komorebi => "komorebi",
            ToolName::Windhawk => "windhawk",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ToolName::Lively => "Lively Wallpaper",
            ToolName::Rainmeter => "Rainmeter",
            ToolName::Yasb => "YASB",
            ToolName::Komorebi => "Komorebi",
            ToolName::Windhawk => "Windhawk",
        }
    }
}

impl std::fmt::Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterCapability {
    Install,
    Detect,
    ThemeTokens,
    WallpaperControl,
    WidgetConfig,
    BarConfig,
    TilingControl,
    ManagedMods,
    BackupRollback,
    LiveReload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterReloadMode {
    None,
    ProcessRestart,
    ConfigTouch,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterSafetyLevel {
    Safe,
    Cautious,
    Curated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterHealth {
    Healthy,
    Missing,
    Degraded,
    NeedsAttention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub tool: ToolName,
    pub display_name: String,
    pub install_source: String,
    pub owned_paths: Vec<String>,
    pub capabilities: Vec<AdapterCapability>,
    pub reload_mode: AdapterReloadMode,
    pub verification_mode: String,
    pub safety_level: AdapterSafetyLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterStatus {
    pub tool: ToolName,
    pub installed: bool,
    pub version: Option<String>,
    pub health: AdapterHealth,
    pub details: String,
    pub metadata: AdapterMetadata,
}
