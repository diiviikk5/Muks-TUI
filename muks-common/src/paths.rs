use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub config_file: PathBuf,
    pub generated: PathBuf,
    pub live: PathBuf,
    pub backups: PathBuf,
    pub presets: PathBuf,
    pub snapshots: PathBuf,
    pub logs: PathBuf,
    pub cache: PathBuf,
}

impl AppPaths {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("failed to resolve home directory")?;
        let root = home.join(".muks");
        Ok(Self {
            config_file: root.join("config.toml"),
            generated: root.join("generated"),
            live: root.join("live"),
            backups: root.join("backups"),
            presets: root.join("presets"),
            snapshots: root.join("snapshots"),
            logs: root.join("logs"),
            cache: root.join("cache"),
            root,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        fs::create_dir_all(&self.generated)
            .with_context(|| format!("failed to create {}", self.generated.display()))?;
        fs::create_dir_all(&self.live)
            .with_context(|| format!("failed to create {}", self.live.display()))?;
        fs::create_dir_all(&self.backups)
            .with_context(|| format!("failed to create {}", self.backups.display()))?;
        fs::create_dir_all(&self.presets)
            .with_context(|| format!("failed to create {}", self.presets.display()))?;
        fs::create_dir_all(&self.snapshots)
            .with_context(|| format!("failed to create {}", self.snapshots.display()))?;
        fs::create_dir_all(&self.logs)
            .with_context(|| format!("failed to create {}", self.logs.display()))?;
        fs::create_dir_all(&self.cache)
            .with_context(|| format!("failed to create {}", self.cache.display()))?;
        Ok(())
    }

    pub fn generated_adapter_dir(&self, adapter: &str) -> PathBuf {
        self.generated.join(adapter)
    }

    pub fn live_adapter_dir(&self, adapter: &str) -> PathBuf {
        self.live.join(adapter)
    }

    pub fn backup_adapter_dir(&self, adapter: &str) -> PathBuf {
        self.backups.join(adapter)
    }
}
