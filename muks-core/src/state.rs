use anyhow::{Context, Result};
use muks_common::AppPaths;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::config::{self, AppConfig};
use crate::snapshot::{self, SnapshotManifest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub profile_name: String,
    pub preset: String,
    pub wallpaper: String,
    pub config_path: String,
    pub snapshot_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReport {
    pub message: String,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeApplyReport {
    pub accent: String,
    pub surface: String,
    pub snapshot_id: Option<String>,
}

pub struct MuksState {
    pub paths: AppPaths,
}

impl MuksState {
    pub fn new() -> Result<Self> {
        let paths = AppPaths::new()?;
        paths.ensure()?;
        let state = Self { paths };
        state.ensure_bootstrap_files()?;
        Ok(state)
    }

    pub fn config(&self) -> Result<AppConfig> {
        config::load_or_create(&self.paths.config_file)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        config::save(&self.paths.config_file, config)
    }

    pub fn status_report(&self) -> Result<StatusReport> {
        let config = self.config()?;
        let snapshots = snapshot::list_snapshots(&self.paths.snapshots)?;
        Ok(StatusReport {
            profile_name: config.profile.name,
            preset: config.theme.preset,
            wallpaper: config.wallpaper.current,
            config_path: self.paths.config_file.display().to_string(),
            snapshot_count: snapshots.len(),
        })
    }

    pub fn set_wallpaper(&self, source: &str) -> Result<AppConfig> {
        let mut config = self.config()?;
        config.wallpaper.current = source.to_string();
        config.wallpaper.source_type =
            if source.starts_with("http://") || source.starts_with("https://") {
                "url".to_string()
            } else if std::path::PathBuf::from(source).exists() {
                "file".to_string()
            } else {
                "preset".to_string()
            };
        self.save_config(&config)?;
        Ok(config)
    }

    pub fn set_theme_preset(&self, preset: &str) -> Result<AppConfig> {
        let mut config = self.config()?;
        config.profile.preset = preset.to_string();
        config.theme.preset = preset.to_string();
        self.save_config(&config)?;
        Ok(config)
    }

    pub fn create_backup(&self, label: &str) -> Result<SnapshotManifest> {
        snapshot::create_snapshot(
            &self.paths.snapshots,
            &self.paths.config_file,
            &self.paths.generated,
            label,
        )
    }

    pub fn restore_backup(&self, snapshot_id: &str) -> Result<SnapshotManifest> {
        snapshot::restore_snapshot(
            snapshot_id,
            &self.paths.snapshots,
            &self.paths.config_file,
            &self.paths.generated,
        )
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotManifest>> {
        snapshot::list_snapshots(&self.paths.snapshots)
    }

    fn ensure_bootstrap_files(&self) -> Result<()> {
        if !self.paths.presets.join("graphite.toml").exists() {
            let content = "[profile]\nname = \"graphite\"\npreset = \"graphite\"\nworkspace_name = \"main\"\n";
            fs::write(self.paths.presets.join("graphite.toml"), content)
                .context("failed to write default preset")?;
        }
        Ok(())
    }
}
