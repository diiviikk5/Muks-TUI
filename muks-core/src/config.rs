use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub profile: ProfileConfig,
    pub wallpaper: WallpaperConfig,
    pub theme: ThemeConfig,
    pub rainmeter: ToolConfig,
    pub yasb: ToolConfig,
    pub komorebi: ToolConfig,
    pub windhawk: ToolConfig,
    pub adapters: AdaptersConfig,
    pub sync: SyncConfig,
    pub install: InstallConfig,
    pub backup: BackupConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            profile: ProfileConfig::default(),
            wallpaper: WallpaperConfig::default(),
            theme: ThemeConfig::default(),
            rainmeter: ToolConfig::enabled("default"),
            yasb: ToolConfig::enabled("default"),
            komorebi: ToolConfig::enabled("default"),
            windhawk: ToolConfig::enabled("curated"),
            adapters: AdaptersConfig::default(),
            sync: SyncConfig::default(),
            install: InstallConfig::default(),
            backup: BackupConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileConfig {
    pub name: String,
    pub preset: String,
    pub workspace_name: String,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            preset: "graphite".to_string(),
            workspace_name: "main".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub current: String,
    pub source_type: String,
    pub fit_mode: String,
    pub auto_extract_palette: bool,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            current: "nebula".to_string(),
            source_type: "preset".to_string(),
            fit_mode: "fill".to_string(),
            auto_extract_palette: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: String,
    pub accent_mode: String,
    pub sync_all_adapters: bool,
    pub reduced_motion: bool,
    pub accent_override: Option<String>,
    pub accent_soft_override: Option<String>,
    pub background_override: Option<String>,
    pub surface_override: Option<String>,
    pub text_override: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: "graphite".to_string(),
            accent_mode: "palette".to_string(),
            sync_all_adapters: true,
            reduced_motion: false,
            accent_override: None,
            accent_soft_override: None,
            background_override: None,
            surface_override: None,
            text_override: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolConfig {
    pub enabled: bool,
    pub profile: String,
    pub managed_path: Option<String>,
}

impl ToolConfig {
    pub fn enabled(profile: &str) -> Self {
        Self {
            enabled: true,
            profile: profile.to_string(),
            managed_path: None,
        }
    }
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self::enabled("default")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdaptersConfig {
    pub auto_detect: bool,
    pub strict_verification: bool,
    pub allow_best_effort: bool,
}

impl Default for AdaptersConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            strict_verification: true,
            allow_best_effort: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub watch_enabled: bool,
    pub debounce_ms: u64,
    pub live_apply: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            watch_enabled: true,
            debounce_ms: 700,
            live_apply: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallConfig {
    pub source_policy: String,
    pub prefer_winget: bool,
    pub cache_downloads: bool,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            source_policy: "official-only".to_string(),
            prefer_winget: true,
            cache_downloads: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    pub auto_snapshot_before_apply: bool,
    pub keep_last: usize,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            auto_snapshot_before_apply: true,
            keep_last: 20,
        }
    }
}

pub fn load_or_create(path: &Path) -> Result<AppConfig> {
    if path.exists() {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
        return Ok(config);
    }

    let config = AppConfig::default();
    save(path, &config)?;
    Ok(config)
}

pub fn save(path: &Path, config: &AppConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let encoded = toml::to_string_pretty(config).context("failed to encode config")?;
    fs::write(path, encoded).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_enabled_for_all_managed_tools() {
        let config = AppConfig::default();
        assert!(config.rainmeter.enabled);
        assert!(config.yasb.enabled);
        assert!(config.komorebi.enabled);
        assert!(config.windhawk.enabled);
    }
}
